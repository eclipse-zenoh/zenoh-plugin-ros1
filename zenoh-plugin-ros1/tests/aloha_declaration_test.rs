//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

use std::{
    collections::HashSet,
    str::FromStr,
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use test_case::test_case;
use tokio::sync::Mutex;
use zenoh::{key_expr::OwnedKeyExpr, session::OpenBuilder, Result as ZResult, Session, Wait};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    aloha_declaration, aloha_subscription, test_helpers::IsolatedConfig,
};

const TIMEOUT: Duration = Duration::from_secs(30);

fn session_builder(cfg: &IsolatedConfig) -> OpenBuilder<zenoh::config::Config> {
    zenoh::open(cfg.peer())
}

fn declaration_builder(
    session: Session,
    beacon_period: Duration,
) -> aloha_declaration::AlohaDeclaration {
    aloha_declaration::AlohaDeclaration::new(
        session,
        zenoh::key_expr::OwnedKeyExpr::from_str("key").unwrap(),
        beacon_period,
    )
}

fn subscription_builder(
    session: Session,
    beacon_period: Duration,
) -> aloha_subscription::AlohaSubscriptionBuilder {
    aloha_subscription::AlohaSubscriptionBuilder::new(
        session,
        zenoh::key_expr::OwnedKeyExpr::from_str("key").unwrap(),
        beacon_period,
    )
}

fn make_session(cfg: &IsolatedConfig) -> Session {
    session_builder(cfg).wait().unwrap()
}

async fn make_subscription(
    session: Session,
    beacon_period: Duration,
) -> aloha_subscription::AlohaSubscription {
    subscription_builder(session, beacon_period)
        .build()
        .await
        .expect("Failed to make subscription")
}

#[tokio::test(flavor = "multi_thread")]
async fn aloha_instantination_one_instance() {
    let session = make_session(&IsolatedConfig::default());
    let _declaration = declaration_builder(session.clone(), Duration::from_secs(1));
    let _subscription = make_subscription(session, Duration::from_secs(1)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn aloha_instantination_many_instances() {
    let cfg = IsolatedConfig::default();
    let mut sessions = Vec::new();
    let mut declarations = Vec::new();
    let mut subscriptions = Vec::new();
    for _ in 0..10 {
        let session = make_session(&cfg);
        sessions.push(session.clone());
        declarations.push(declaration_builder(session.clone(), Duration::from_secs(1)));
    }

    for session in sessions.iter() {
        subscriptions.push(make_subscription(session.clone(), Duration::from_secs(1)).await);
    }
}

pub struct PPCMeasurement {
    _subscriber: zenoh::pubsub::Subscriber<()>,
    ppc: Arc<AtomicUsize>,
    measurement_period: Duration,
}
impl PPCMeasurement {
    pub async fn new(
        session: &Session,
        key: String,
        measurement_period: Duration,
    ) -> ZResult<PPCMeasurement> {
        let ppc = Arc::new(AtomicUsize::new(0));
        let p = ppc.clone();
        let subscriber = session
            .declare_subscriber(key)
            .callback(move |_val| {
                p.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .await?;

        Ok(Self {
            _subscriber: subscriber,
            ppc,
            measurement_period,
        })
    }

    pub async fn measure_ppc(&self) -> usize {
        self.ppc.store(0, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(self.measurement_period).await;
        self.ppc.load(std::sync::atomic::Ordering::SeqCst)
    }
}

struct DeclarationCollector {
    resources: Arc<Mutex<HashSet<zenoh::key_expr::OwnedKeyExpr>>>,

    to_be_declared: Arc<Mutex<HashSet<zenoh::key_expr::OwnedKeyExpr>>>,
    to_be_undeclared: Arc<Mutex<HashSet<zenoh::key_expr::OwnedKeyExpr>>>,
}
impl DeclarationCollector {
    fn new() -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashSet::new())),
            to_be_declared: Arc::new(Mutex::new(HashSet::new())),
            to_be_undeclared: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn use_builder(
        &self,
        mut builder: aloha_subscription::AlohaSubscriptionBuilder,
    ) -> aloha_subscription::AlohaSubscriptionBuilder {
        let r = self.resources.clone();
        let r2 = r.clone();

        let declared = self.to_be_declared.clone();
        let undeclared = self.to_be_undeclared.clone();

        builder = builder
            .on_resource_declared(move |k| {
                let declared = declared.clone();
                let r = r.clone();
                let k_owned = OwnedKeyExpr::from(k);
                Box::new(Box::pin(async move {
                    assert!(declared.lock().await.remove::<OwnedKeyExpr>(&k_owned));
                    assert!(r.lock().await.insert(k_owned));
                }))
            })
            .on_resource_undeclared(move |k| {
                let undeclared = undeclared.clone();
                let r2 = r2.clone();
                let k_owned = OwnedKeyExpr::from(k);
                Box::new(Box::pin(async move {
                    assert!(undeclared.lock().await.remove(&k_owned));
                    assert!(r2.lock().await.remove(&k_owned));
                }))
            });

        builder
    }

    pub async fn arm(
        &mut self,
        declared: HashSet<zenoh::key_expr::OwnedKeyExpr>,
        undeclared: HashSet<zenoh::key_expr::OwnedKeyExpr>,
    ) {
        *self.to_be_declared.lock().await = declared;
        *self.to_be_undeclared.lock().await = undeclared;
    }

    pub async fn wait(&self, expected: HashSet<zenoh::key_expr::OwnedKeyExpr>) {
        while !self.to_be_declared.lock().await.is_empty()
            || !self.to_be_undeclared.lock().await.is_empty()
            || expected != *self.resources.lock().await
        {
            tokio::time::sleep(core::time::Duration::from_millis(1)).await;
        }
    }
}

#[derive(Default)]
struct State {
    pub declarators_count: usize,
}
impl State {
    pub fn declarators(mut self, declarators_count: usize) -> Self {
        self.declarators_count = declarators_count;
        self
    }
}

async fn test_state_transition(
    cfg: &IsolatedConfig,
    beacon_period: Duration,
    declaring_sessions: &mut Vec<Session>,
    declarations: &mut Vec<aloha_declaration::AlohaDeclaration>,
    collector: &mut DeclarationCollector,
    ppc_measurer: &PPCMeasurement,
    state: &State,
) {
    let ke = zenoh::key_expr::OwnedKeyExpr::from_str("key").unwrap();
    let mut result: HashSet<zenoh::key_expr::OwnedKeyExpr> = HashSet::new();
    let mut undeclared: HashSet<zenoh::key_expr::OwnedKeyExpr> = HashSet::new();
    let mut declared: HashSet<zenoh::key_expr::OwnedKeyExpr> = HashSet::new();

    match (declarations.len(), state.declarators_count) {
        (0, 0) => {}
        (0, _) => {
            result.insert(ke.clone());
            declared.insert(ke.clone());
        }
        (_, 0) => {
            undeclared.insert(ke.clone());
        }
        (_, _) => {
            result.insert(ke.clone());
        }
    }

    collector.arm(declared, undeclared).await;

    while declarations.len() > state.declarators_count {
        declarations.pop();
    }

    while declarations.len() < state.declarators_count {
        if declaring_sessions.len() <= declarations.len() {
            declaring_sessions.push(session_builder(cfg).await.unwrap());
        }
        declarations.push(declaration_builder(
            declaring_sessions[declarations.len()].clone(),
            beacon_period,
        ));
    }

    collector.wait(result).await;
    tokio::time::sleep(beacon_period).await;
    while ppc_measurer.measure_ppc().await != {
        let mut res = 1;
        if state.declarators_count == 0 {
            res = 0;
        }
        res
    } {}
}

async fn run_aloha(beacon_period: Duration, scenario: Vec<State>) {
    let cfg = IsolatedConfig::default();
    let mut declaring_sessions: Vec<Session> = Vec::new();
    let mut declarations: Vec<aloha_declaration::AlohaDeclaration> = Vec::new();

    let mut collector = DeclarationCollector::new();
    let subscription_session = session_builder(&cfg).await.unwrap();
    let _subscriber = collector
        .use_builder(subscription_builder(
            subscription_session.clone(),
            beacon_period,
        ))
        .build()
        .await
        .unwrap();
    let ppc_measurer = PPCMeasurement::new(&subscription_session, "key".to_string(), beacon_period)
        .await
        .unwrap();
    for scene in scenario {
        println!("Transiting State: {}", scene.declarators_count);
        tokio::time::timeout(
            TIMEOUT,
            test_state_transition(
                &cfg,
                beacon_period,
                &mut declaring_sessions,
                &mut declarations,
                &mut collector,
                &ppc_measurer,
                &scene,
            ),
        )
        .await
        .expect("Timeout waiting state transition!");
    }
}

#[test_case([State::default().declarators(1)].into_iter().collect(); "one")]
#[test_case([State::default().declarators(10)].into_iter().collect(); "many")]
#[test_case([State::default().declarators(10),
             State::default().declarators(1),
             State::default().declarators(10)].into_iter().collect(); "many one many")]
#[test_case([State::default().declarators(1),
             State::default().declarators(0),
             State::default().declarators(1)].into_iter().collect(); "one zero one")]
#[test_case([State::default().declarators(10),
             State::default().declarators(0),
             State::default().declarators(10)].into_iter().collect(); "many zero many")]
#[test_case([State::default().declarators(1),
             State::default().declarators(10),
             State::default().declarators(1),
             State::default().declarators(10),
             State::default().declarators(1),
             State::default().declarators(10),
             State::default().declarators(0),
             State::default().declarators(1),
             State::default().declarators(10),
             State::default().declarators(1),
             State::default().declarators(0),
             State::default().declarators(10),
             State::default().declarators(1)].into_iter().collect(); "many scenarios")]
#[tokio::test(flavor = "multi_thread")]
async fn aloha_declare(vec_state: Vec<State>) {
    run_aloha(Duration::from_millis(100), vec_state).await;
}
