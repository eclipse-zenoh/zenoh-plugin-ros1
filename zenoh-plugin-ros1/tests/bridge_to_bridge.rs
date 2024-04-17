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

use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashSet, sync::atomic::AtomicU64};

use async_std::prelude::FutureExt;
use rosrust::RawMessage;
use std::sync::atomic::{AtomicUsize, Ordering::*};
use strum_macros::Display;
use tracing::{debug, trace};
use zenoh::prelude::{KeyExpr, OwnedKeyExpr};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::test_helpers::{
    self, wait_async, Publisher, Subscriber,
};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    bridging_mode::BridgingMode,
    environment::Environment,
    test_helpers::{
        BridgeChecker, IsolatedConfig, IsolatedROSMaster, ROSEnvironment, RunningBridge, TestParams,
    },
};

struct ROSWithChecker {
    _ros_env: ROSEnvironment,
    pub checker: BridgeChecker,
}
impl ROSWithChecker {
    pub fn new(zenoh_cfg: zenoh::config::Config, ros_cfg: &IsolatedROSMaster) -> Self {
        ROSWithChecker {
            _ros_env: ROSEnvironment::new(ros_cfg.port.port).with_master(),
            checker: BridgeChecker::new(zenoh_cfg, &ros_cfg.master_uri()),
        }
    }
}

struct ROSSystem<'a> {
    zenoh_cfg: &'a IsolatedConfig,
    cfg: IsolatedROSMaster,
    ros_env: Option<ROSWithChecker>,
    bridge: Option<RunningBridge>,
}
impl<'a> ROSSystem<'a> {
    fn new(zenoh_cfg: &'a IsolatedConfig) -> Self {
        Self {
            zenoh_cfg,
            cfg: IsolatedROSMaster::default(),
            ros_env: None,
            bridge: None,
        }
    }

    fn with_ros(&mut self) -> &mut Self {
        match self.ros_env {
            Some(_) => {
                panic!("Trying to start ros env when it already exists!");
            }
            None => {
                let _ = self
                    .ros_env
                    .insert(ROSWithChecker::new(self.zenoh_cfg.peer(), &self.cfg));
            }
        }
        self
    }
    fn without_ros(&mut self) -> &mut Self {
        if self.ros_env.take().is_none() {
            panic!("Trying to stop ros env when it does not exist!");
        }
        self
    }
    fn with_bridge(&mut self) -> &mut Self {
        match self.bridge {
            Some(_) => {
                panic!("Trying to start bridge when it already exists!");
            }
            None => {
                let _ = self.bridge.insert(RunningBridge::new(
                    self.zenoh_cfg.peer(),
                    self.cfg.master_uri(),
                ));
            }
        }
        self
    }
    fn without_bridge(&mut self) -> &mut Self {
        if self.bridge.take().is_none() {
            panic!("Trying to stop bridge when it does not exist!");
        }
        self
    }
    async fn wait_state_synch(&self) {
        match (&self.ros_env, &self.bridge) {
            (None, Some(bridge)) => {
                bridge.assert_ros_error().await;
                bridge.assert_bridge_empy().await;
            }
            (Some(ros), Some(bridge)) => {
                bridge.assert_ros_ok().await;
                bridge
                    .assert_bridge_status(|| *ros.checker.expected_bridge_status.read().unwrap())
                    .await;
            }
            (_, _) => {}
        }
    }
}

#[derive(Default)]
struct TestEnvironment {
    cfg: IsolatedConfig,
}
impl TestEnvironment {
    fn add_system(&self) -> ROSSystem {
        ROSSystem::new(&self.cfg)
    }
}

async fn async_create_bridge() {
    let env = TestEnvironment::default();
    let system1 = env.add_system();
    system1.wait_state_synch().await;
}

#[test]
fn create_bridge() {
    async_std::task::block_on(async_create_bridge());
}

async fn async_create_bridge_and_init() {
    let env = TestEnvironment::default();
    let mut system1 = env.add_system();
    system1.with_ros().with_bridge().wait_state_synch().await;
}

#[test]
fn create_bridge_and_init() {
    async_std::task::block_on(async_create_bridge_and_init());
}

async fn async_create_bridge_and_reinit_ros() {
    let env = TestEnvironment::default();
    let mut system1 = env.add_system();
    system1.with_bridge().wait_state_synch().await;

    for _ in 0..10 {
        system1.with_ros().wait_state_synch().await;
        system1.without_ros().wait_state_synch().await;
    }
}

#[test]
fn create_bridge_and_reinit_ros() {
    async_std::task::block_on(async_create_bridge_and_reinit_ros());
}

async fn async_create_bridge_and_reinit_bridge() {
    let env = TestEnvironment::default();
    let mut system1 = env.add_system();
    system1.with_ros().wait_state_synch().await;

    for _ in 0..10 {
        system1.with_bridge().wait_state_synch().await;
        system1.without_bridge().wait_state_synch().await;
    }
}

#[test]
fn create_bridge_and_reinit_bridge() {
    async_std::task::block_on(async_create_bridge_and_reinit_bridge());
}

struct SrcDstPair {
    key: OwnedKeyExpr,
    counter: Arc<AtomicUsize>,
    src: Box<dyn Publisher>,
    dst: Box<dyn Subscriber>,
}

impl SrcDstPair {
    pub async fn run(self, pps_measurements: u32) {
        self.start().await;
        self.check_pps(pps_measurements).await;
    }

    async fn start(&self) {
        self.wait_for_ready().await;
        assert!(self.start_ping_pong().await);
    }

    async fn wait_for_ready(&self) {
        assert!(
            wait_async(
                || async { self.src.ready().await && self.dst.ready() },
                core::time::Duration::from_secs(30)
            )
            .await
        );
    }

    async fn start_ping_pong(&self) -> bool {
        debug!("Starting ping-pong!");
        let mut data = Vec::new();
        data.reserve(TestParams::data_size() as usize);
        for i in 0..TestParams::data_size() {
            data.push((i % 255) as u8);
        }

        async {
            while {
                self.src.put(data.clone());
                !test_helpers::wait_async_fn(
                    || self.counter.load(Relaxed) > 0,
                    Duration::from_secs(5),
                )
                .await
            } {
                debug!("Restarting ping-pong!");
            }
        }
        .timeout(Duration::from_secs(30))
        .await
        .is_ok()
    }

    async fn check_pps(&self, pps_measurements: u32) {
        for i in 0..pps_measurements {
            let pps = self.measure_pps().await;
            trace!("PPS #{}: {} \t Key: {}", i, pps, self.key);
            assert!(pps > 0.0);
        }
    }

    async fn measure_pps(&self) -> f64 {
        debug!("Starting measure PPS....");

        let duration_milliseconds = TestParams::pps_measure_period_ms();

        let mut result = 0.0;
        let mut duration: u64 = 0;

        self.counter.store(0, Relaxed);
        while !(result > 0.0 || duration >= 10000) {
            async_std::task::sleep(core::time::Duration::from_millis(duration_milliseconds)).await;
            duration += duration_milliseconds;
            result += self.counter.load(Relaxed) as f64;
        }
        debug!("...finished measure PPS!");
        result * 1000.0 / (duration as f64)
    }
}

#[derive(PartialEq, Eq, Hash, Display)]
enum Mode {
    Ros1ToZenoh,
    ZenohToRos1,
    Ros1Service,
    Ros1Client,
}
static UNIQUE_NUMBER: AtomicU64 = AtomicU64::new(0);
async fn async_bridge_2_bridge(instances: u32, mode: std::collections::HashSet<Mode>) {
    let create_pubsub = |topic: KeyExpr,
                         pub_checker: &BridgeChecker,
                         sub_checker: &BridgeChecker| {
        let counter = Arc::new(AtomicUsize::new(0));
        let publisher = Arc::new(pub_checker.make_ros_publisher(topic.as_str()));

        let c_counter = counter.clone();
        let c_publisger = publisher.clone();
        let subscriber = sub_checker.make_ros_subscriber(topic.as_str(), move |msg: RawMessage| {
            c_counter.fetch_add(1, SeqCst);
            c_publisger.data.send(msg).unwrap();
        });

        let src = Box::new(test_helpers::ROS1Publisher { inner: publisher });
        let dst = Box::new(test_helpers::ROS1Subscriber { inner: subscriber });

        SrcDstPair {
            key: topic.into(),
            counter,
            src,
            dst,
        }
    };

    let create_srv_client =
        |topic: KeyExpr, srv_checker: &BridgeChecker, client_checker: &BridgeChecker| {
            let counter = Arc::new(AtomicUsize::new(0));

            let service = srv_checker.make_ros_service(topic.as_str(), Ok);

            let topic_descriptor = BridgeChecker::make_topic(&topic);
            let src = Box::new(test_helpers::ROS1Client::new(
                client_checker,
                topic_descriptor,
                counter.clone(),
            ));
            let dst = Box::new(test_helpers::ROS1Service { _inner: service });

            SrcDstPair {
                key: topic.into(),
                counter,
                src,
                dst,
            }
        };

    zenoh_core::zasync_executor_init!();

    let env = TestEnvironment::default();
    let mut src_system = env.add_system();
    src_system.with_ros().with_bridge();

    let mut dst_system = env.add_system();
    dst_system.with_ros().with_bridge();

    let make_keyexpr = |i: u32, mode: Mode| -> KeyExpr {
        format!(
            "some/key/expr{}_{}_{}",
            i,
            mode,
            UNIQUE_NUMBER.fetch_add(1, SeqCst)
        )
        .try_into()
        .unwrap()
    };

    // wait for systems to become ready
    src_system.wait_state_synch().await;
    dst_system.wait_state_synch().await;

    let src_checker = &src_system.ros_env.as_ref().unwrap().checker;
    let dst_checker = &dst_system.ros_env.as_ref().unwrap().checker;

    // create topic entities
    let mut entities = Vec::new();
    for i in 0..instances {
        if mode.contains(&Mode::Ros1ToZenoh) {
            let topic = make_keyexpr(i, Mode::Ros1ToZenoh);
            let entity = create_pubsub(topic, src_checker, dst_checker);
            entities.push(entity);
        }
        if mode.contains(&Mode::ZenohToRos1) {
            let topic = make_keyexpr(i, Mode::ZenohToRos1);
            let entity = create_pubsub(topic, dst_checker, src_checker);
            entities.push(entity);
        }
        if mode.contains(&Mode::Ros1Service) {
            // allow automatical client bridging because it is disabled by default
            Environment::client_bridging_mode().set(BridgingMode::Auto);

            let topic = make_keyexpr(i, Mode::Ros1Service);
            let entity = create_srv_client(topic, src_checker, dst_checker);
            entities.push(entity);
        }
        if mode.contains(&Mode::Ros1Client) {
            // allow automatical client bridging because it is disabled by default
            Environment::client_bridging_mode().set(BridgingMode::Auto);

            let topic = make_keyexpr(i, Mode::Ros1Client);
            let entity = create_srv_client(topic, dst_checker, src_checker);
            entities.push(entity);
        }
    }

    // check states
    src_system.wait_state_synch().await;
    dst_system.wait_state_synch().await;

    // run all entities
    let mut vec = Vec::new();
    for entity in entities {
        vec.push(entity.run(TestParams::pps_measurements()));
    }
    futures::future::join_all(vec).await;

    // check states again
    src_system.wait_state_synch().await;
    dst_system.wait_state_synch().await;
}

#[test]
fn bridge_2_bridge_pub() {
    async_std::task::block_on(async_bridge_2_bridge(1, HashSet::from([Mode::Ros1ToZenoh])));
}

#[test]
fn bridge_2_bridge_pub_many() {
    async_std::task::block_on(async_bridge_2_bridge(
        TestParams::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
}

#[test]
fn bridge_2_bridge_sub() {
    async_std::task::block_on(async_bridge_2_bridge(1, HashSet::from([Mode::ZenohToRos1])));
}

#[test]
fn bridge_2_bridge_sub_many() {
    async_std::task::block_on(async_bridge_2_bridge(
        TestParams::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    ));
}

#[test]
fn bridge_2_bridge_service() {
    async_std::task::block_on(async_bridge_2_bridge(1, HashSet::from([Mode::Ros1Service])));
}

#[test]
fn bridge_2_bridge_service_many() {
    async_std::task::block_on(async_bridge_2_bridge(
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Service]),
    ));
}

#[test]
fn bridge_2_bridge_client() {
    async_std::task::block_on(async_bridge_2_bridge(1, HashSet::from([Mode::Ros1Client])));
}

#[test]
fn bridge_2_bridge_client_many() {
    async_std::task::block_on(async_bridge_2_bridge(
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Client]),
    ));
}

#[test]
fn bridge_2_bridge_all() {
    async_std::task::block_on(async_bridge_2_bridge(
        1,
        HashSet::from([
            Mode::Ros1Client,
            Mode::Ros1Service,
            Mode::Ros1ToZenoh,
            Mode::ZenohToRos1,
        ]),
    ));
}

#[test]
fn bridge_2_bridge_all_many() {
    async_std::task::block_on(async_bridge_2_bridge(
        TestParams::many_count(),
        HashSet::from([
            Mode::Ros1Client,
            Mode::Ros1Service,
            Mode::Ros1ToZenoh,
            Mode::ZenohToRos1,
        ]),
    ));
}
