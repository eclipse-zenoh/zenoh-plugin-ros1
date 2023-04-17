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
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_std::prelude::FutureExt;
use multiset::HashMultiSet;
use serial_test::serial;
use zenoh::{config::ModeDependentValue, prelude::keyexpr, OpenBuilder, Session};
use zenoh_core::{AsyncResolve, SyncResolve};
use zplugin_ros1::ros_to_zenoh_bridge::{discovery, topic_utilities::make_topic};

const TIMEOUT: Duration = Duration::from_secs(10);

fn session_builder() -> OpenBuilder<zenoh::config::Config> {
    let mut config = zenoh::config::peer();
    config
        .scouting
        .multicast
        .set_address(Some(SocketAddr::from_str("224.0.0.224:16001").unwrap()))
        .unwrap();
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();
    zenoh::open(config)
}

fn discovery_builder(session: Arc<Session>) -> discovery::DiscoveryBuilder {
    discovery::DiscoveryBuilder::new("*".to_string(), "*".to_string(), session)
}

fn make_session() -> Arc<Session> {
    session_builder().res_sync().unwrap().into_arc()
}

fn make_discovery(session: Arc<Session>) -> discovery::Discovery {
    async_std::task::block_on(discovery_builder(session).build())
}

#[test]
#[serial(ROS1)]
fn discovery_instantination_one_instance() {
    let session = make_session();
    let _discovery = make_discovery(session);
}

#[test]
#[serial(ROS1)]
fn discovery_instantination_many_instances() {
    let mut sessions = Vec::new();
    for _i in 0..10 {
        sessions.push(make_session());
    }

    let mut discoveries = Vec::new();
    for session in sessions.iter() {
        discoveries.push(make_discovery(session.clone()));
    }
}

struct DiscoveryCollector {
    publishers: Arc<Mutex<HashMultiSet<rosrust::api::Topic>>>,
    subscribers: Arc<Mutex<HashMultiSet<rosrust::api::Topic>>>,
    services: Arc<Mutex<HashMultiSet<rosrust::api::Topic>>>,
    clients: Arc<Mutex<HashMultiSet<rosrust::api::Topic>>>,
}
impl DiscoveryCollector {
    fn new() -> Self {
        Self {
            publishers: Arc::new(Mutex::new(HashMultiSet::new())),
            subscribers: Arc::new(Mutex::new(HashMultiSet::new())),
            services: Arc::new(Mutex::new(HashMultiSet::new())),
            clients: Arc::new(Mutex::new(HashMultiSet::new())),
        }
    }

    pub fn use_builder(
        &self,
        mut builder: discovery::DiscoveryBuilder,
    ) -> discovery::DiscoveryBuilder {
        let p = self.publishers.clone();
        let s = self.subscribers.clone();
        let srv = self.services.clone();
        let cli = self.clients.clone();

        let p1 = self.publishers.clone();
        let s1 = self.subscribers.clone();
        let srv1 = self.services.clone();
        let cli1 = self.clients.clone();

        builder
            .on_discovered(move |b_type, topic| {
                let container = match b_type {
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Publisher => &p,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Subscriber => &s,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Service => &srv,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Client => &cli,
                };
                container.lock().unwrap().insert(topic);
                Box::new(Box::pin(async {}))
            })
            .on_lost(move |b_type, topic| {
                let container = match b_type {
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Publisher => &p1,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Subscriber => &s1,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Service => &srv1,
                    zplugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Client => &cli1,
                };
                container.lock().unwrap().remove(&topic);
                Box::new(Box::pin(async {}))
            });

        builder
    }

    pub async fn wait_publishers(&self, expected: HashMultiSet<rosrust::api::Topic>) {
        Self::wait(&self.publishers, expected).await;
    }

    pub async fn wait_subscribers(&self, expected: HashMultiSet<rosrust::api::Topic>) {
        Self::wait(&self.subscribers, expected).await;
    }

    pub async fn wait_services(&self, expected: HashMultiSet<rosrust::api::Topic>) {
        Self::wait(&self.services, expected).await;
    }

    pub async fn wait_clients(&self, expected: HashMultiSet<rosrust::api::Topic>) {
        Self::wait(&self.clients, expected).await;
    }

    // PRIVATE:
    async fn wait(
        container: &Arc<Mutex<HashMultiSet<rosrust::api::Topic>>>,
        expected: HashMultiSet<rosrust::api::Topic>,
    ) {
        while expected != *container.lock().unwrap() {
            async_std::task::sleep(core::time::Duration::from_millis(1)).await;
        }
    }
}

async fn generate_topics(
    count: usize,
    duplication: usize,
    stage: usize,
) -> HashMultiSet<rosrust::api::Topic> {
    let mut result = HashMultiSet::new();
    for number in 0..count {
        for _dup in 0..duplication {
            unsafe {
                let topic = make_topic(
                    keyexpr::from_str_unchecked("some"),
                    keyexpr::from_str_unchecked(format!("name_{}_{}", number, stage).as_str()),
                )
                .unwrap();

                result.insert(topic);
            }
        }
    }
    result
}

#[derive(Default)]
struct State {
    publishers_count: usize,
    publishers_duplication: usize,
    subscribers_count: usize,
    subscribers_duplication: usize,
    services_count: usize,
    services_duplication: usize,
    clients_count: usize,
    clients_duplication: usize,
    stage: usize,
}
impl State {
    pub fn publishers(mut self, publishers_count: usize, publishers_duplication: usize) -> Self {
        self.publishers_count = publishers_count;
        self.publishers_duplication = publishers_duplication;
        self
    }
    pub fn subscribers(mut self, subscribers_count: usize, subscribers_duplication: usize) -> Self {
        self.subscribers_count = subscribers_count;
        self.subscribers_duplication = subscribers_duplication;
        self
    }
    pub fn services(mut self, services_count: usize, services_duplication: usize) -> Self {
        self.services_count = services_count;
        self.services_duplication = services_duplication;
        self
    }
    pub fn clients(mut self, clients_count: usize, clients_duplication: usize) -> Self {
        self.clients_count = clients_count;
        self.clients_duplication = clients_duplication;
        self
    }
    //pub fn stage(mut self, stage: usize) -> Self {
    //    self.stage = stage;
    //    self
    //}
    async fn summarize(
        &self,
    ) -> (
        HashMultiSet<rosrust::api::Topic>,
        HashMultiSet<rosrust::api::Topic>,
        HashMultiSet<rosrust::api::Topic>,
        HashMultiSet<rosrust::api::Topic>,
    ) {
        (
            generate_topics(
                self.publishers_count,
                self.publishers_duplication,
                self.stage,
            )
            .await,
            generate_topics(
                self.subscribers_count,
                self.subscribers_duplication,
                self.stage,
            )
            .await,
            generate_topics(self.services_count, self.services_duplication, self.stage).await,
            generate_topics(self.clients_count, self.clients_duplication, self.stage).await,
        )
    }
}

async fn test_state_transition(
    src_discovery: &discovery::Discovery,
    rcv: &DiscoveryCollector,
    state: &State,
) {
    let (publishers, subscribers, services, clients) = state.summarize().await;

    let mut _pub_entities = Vec::new();
    for publisher in publishers.iter() {
        _pub_entities.push(
            src_discovery
                .local_resources()
                .declare_publisher(publisher)
                .await,
        );
    }

    let mut _sub_entities = Vec::new();
    for subscriber in subscribers.iter() {
        _sub_entities.push(
            src_discovery
                .local_resources()
                .declare_subscriber(subscriber)
                .await,
        );
    }

    let mut _srv_entities = Vec::new();
    for service in services.iter() {
        _srv_entities.push(
            src_discovery
                .local_resources()
                .declare_service(service)
                .await,
        );
    }

    let mut _cl_entities = Vec::new();
    for client in clients.iter() {
        _cl_entities.push(src_discovery.local_resources().declare_client(client).await);
    }

    rcv.wait_publishers(publishers).await;
    rcv.wait_subscribers(subscribers).await;
    rcv.wait_services(services).await;
    rcv.wait_clients(clients).await;
}

async fn run_discovery(scenario: Vec<State>) {
    let src_session = session_builder().res_async().await.unwrap().into_arc();
    let src_discovery = discovery_builder(src_session).build().await;

    let rcv = DiscoveryCollector::new();
    let rcv_session = session_builder().res_async().await.unwrap().into_arc();
    let _rcv_discovery = rcv
        .use_builder(discovery_builder(rcv_session))
        .build()
        .await;

    for scene in scenario {
        test_state_transition(&src_discovery, &rcv, &scene)
            .timeout(TIMEOUT)
            .await
            .expect("Timeout waiting state transition!");
    }
}

#[test]
#[serial(ROS1)]
fn discover_single_publisher() {
    async_std::task::block_on(run_discovery(
        [State::default().publishers(1, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_single_subscriber() {
    async_std::task::block_on(run_discovery(
        [State::default().subscribers(1, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_single_service() {
    async_std::task::block_on(run_discovery(
        [State::default().services(1, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_single_client() {
    async_std::task::block_on(run_discovery(
        [State::default().clients(1, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_single_transition() {
    async_std::task::block_on(run_discovery(
        [
            State::default().publishers(1, 1),
            State::default().subscribers(1, 1),
            State::default().services(1, 1),
            State::default().clients(1, 1),
        ]
        .into_iter()
        .collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_single_transition_with_zero_state() {
    async_std::task::block_on(run_discovery(
        [
            State::default().publishers(1, 1),
            State::default(),
            State::default().subscribers(1, 1),
            State::default(),
            State::default().services(1, 1),
            State::default(),
            State::default().clients(1, 1),
        ]
        .into_iter()
        .collect(),
    ));
}

#[test]
#[serial(ROS1)]
fn discover_multiple_publishers() {
    async_std::task::block_on(run_discovery(
        [State::default().publishers(100, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_multiple_subscribers() {
    async_std::task::block_on(run_discovery(
        [State::default().subscribers(100, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_multiple_services() {
    async_std::task::block_on(run_discovery(
        [State::default().services(100, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_multiple_clients() {
    async_std::task::block_on(run_discovery(
        [State::default().clients(100, 1)].into_iter().collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_multiple_transition() {
    async_std::task::block_on(run_discovery(
        [
            State::default().publishers(100, 1),
            State::default().subscribers(100, 1),
            State::default().services(100, 1),
            State::default().clients(100, 1),
        ]
        .into_iter()
        .collect(),
    ));
}
#[test]
#[serial(ROS1)]
fn discover_multiple_transition_with_zero_state() {
    async_std::task::block_on(run_discovery(
        [
            State::default().publishers(100, 1),
            State::default(),
            State::default().subscribers(100, 1),
            State::default(),
            State::default().services(100, 1),
            State::default(),
            State::default().clients(100, 1),
        ]
        .into_iter()
        .collect(),
    ));
}
