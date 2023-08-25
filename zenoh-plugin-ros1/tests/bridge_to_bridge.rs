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

use std::{collections::HashSet, sync::atomic::AtomicU64};

use rosrust::{Client, Publisher, RawMessage, Service, Subscriber};
use std::sync::atomic::Ordering::*;
use strum_macros::Display;
use zenoh::prelude::KeyExpr;
use zenoh_plugin_ros1::ros_to_zenoh_bridge::test_helpers::{
    BridgeChecker, IsolatedConfig, IsolatedROSMaster, RAIICounter, ROSEnvironment, RunningBridge,
    TestParams,
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

#[derive(PartialEq, Eq, Hash, Display)]
enum Mode {
    Ros1ToZenoh,
    ZenohToRos1,
    Ros1Service,
    Ros1Client,
}
static UNIQUE_NUMBER: AtomicU64 = AtomicU64::new(0);
async fn async_bridge_2_bridge(instances: u32, mode: std::collections::HashSet<Mode>) {
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

    enum Entity {
        Pub(RAIICounter<Publisher<RawMessage>>),
        Sub(RAIICounter<Subscriber>),
        Service(RAIICounter<Service>),
        Client(RAIICounter<Client<RawMessage>>),
    }

    // create topic entities
    let mut entities = Vec::new();
    for i in 0..instances {
        if mode.contains(&Mode::Ros1ToZenoh) {
            let topic = make_keyexpr(i, Mode::Ros1ToZenoh);
            entities.push(Entity::Pub(src_checker.make_ros_publisher(topic.as_str())));
            entities.push(Entity::Sub(
                dst_checker.make_ros_subscriber(topic.as_str(), |_: RawMessage| {}),
            ));
        }
        if mode.contains(&Mode::ZenohToRos1) {
            let topic = make_keyexpr(i, Mode::ZenohToRos1);
            entities.push(Entity::Sub(
                src_checker.make_ros_subscriber(topic.as_str(), |_: RawMessage| {}),
            ));
            entities.push(Entity::Pub(dst_checker.make_ros_publisher(topic.as_str())));
        }
        if mode.contains(&Mode::Ros1Service) {
            let topic = make_keyexpr(i, Mode::Ros1Service);
            entities.push(Entity::Service(
                src_checker.make_ros_service(topic.as_str(), Ok),
            ));
            entities.push(Entity::Client(dst_checker.make_ros_client(topic.as_str())));
        }
        if mode.contains(&Mode::Ros1Client) {
            let topic = make_keyexpr(i, Mode::Ros1Client);
            entities.push(Entity::Client(src_checker.make_ros_client(topic.as_str())));
            entities.push(Entity::Service(
                dst_checker.make_ros_service(topic.as_str(), Ok),
            ));
        }
    }

    // check states
    src_system.wait_state_synch().await;
    dst_system.wait_state_synch().await;

    // drop everything
    drop(entities);

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
