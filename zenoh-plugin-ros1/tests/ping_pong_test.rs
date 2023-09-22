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

use strum_macros::Display;
use zenoh::prelude::SplitBuffer;
use zenoh_core::SyncResolve;

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::*},
        Arc,
    },
};

use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    bridging_mode::BridgingMode,
    environment::Environment,
    test_helpers::{
        BridgeChecker, Publisher, ROS1Client, ROS1Publisher, ROS1Service, ROS1Subscriber,
        ROSEnvironment, RunningBridge, Subscriber, TestParams, ZenohPublisher, ZenohQuery,
        ZenohQueryable, ZenohSubscriber,
    },
};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    discovery::LocalResource,
    test_helpers::{IsolatedConfig, IsolatedROSMaster},
};

use log::{debug, trace};
use std::sync::atomic::AtomicUsize;
use zenoh::prelude::r#async::*;

use std::{thread, time};

#[test]
fn env_checks_no_master_init_and_exit_immed() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_no_master_init_and_wait() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_with_master_init_and_exit_immed() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_with_master_init_and_wait() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());

    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_with_master_init_and_loose_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut _ros_env = Some(ROSEnvironment::new(roscfg.port.port).with_master());
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
    _ros_env = None;
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_with_master_init_and_wait_for_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_error());
    async_std::task::block_on(bridge.assert_bridge_empy());
    _ros_env = _ros_env.with_master();
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
    thread::sleep(time::Duration::from_secs(1));
    async_std::task::block_on(bridge.assert_ros_ok());
    async_std::task::block_on(bridge.assert_bridge_empy());
}

#[test]
fn env_checks_with_master_init_and_reconnect_many_times_to_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    for _i in 0..20 {
        async_std::task::block_on(bridge.assert_ros_error());
        async_std::task::block_on(bridge.assert_bridge_empy());
        ros_env = ros_env.with_master();
        async_std::task::block_on(bridge.assert_ros_ok());
        async_std::task::block_on(bridge.assert_bridge_empy());
        ros_env = ros_env.without_master();
    }
}

struct PubSub {
    key: String,
    publisher: Box<dyn Publisher>,
    subscriber: Box<dyn Subscriber>,
    _discovery_resource: LocalResource,
}

struct PingPong {
    pub_sub: PubSub,
    cycles: Arc<AtomicUsize>,
}
impl PingPong {
    pub async fn run(self, pps_measurements: u32) {
        self.start().await;
        self.check_pps(pps_measurements).await;
    }

    // PRIVATE:
    async fn new_ros1_to_zenoh_service(key: &str, backend: Arc<BridgeChecker>) -> PingPong {
        let cycles = Arc::new(AtomicUsize::new(0));
        let ros1_service =
            backend.make_ros_service(key, |q| -> rosrust::ServiceResult<rosrust::RawMessage> {
                debug!(
                    "PingPong: got query of {} bytes from Zenoh to ROS1!",
                    q.0.len()
                );
                Ok(q) // echo the request back!
            });

        let discovery_resource = backend
            .local_resources
            .declare_client(&BridgeChecker::make_topic(key))
            .await
            .unwrap();
        let zenoh_query = ZenohQuery::new(backend, key.to_string(), cycles.clone());

        PingPong {
            pub_sub: PubSub {
                key: key.to_string(),
                publisher: Box::new(zenoh_query),
                subscriber: Box::new(ROS1Service {
                    _inner: ros1_service,
                }),
                _discovery_resource: discovery_resource,
            },
            cycles,
        }
    }

    async fn new_ros1_to_zenoh_client(key: &str, backend: Arc<BridgeChecker>) -> PingPong {
        let cycles = Arc::new(AtomicUsize::new(0));

        let topic = BridgeChecker::make_topic(key);

        let discovery_resource = backend
            .local_resources
            .declare_service(&topic)
            .await
            .unwrap();
        let zenoh_queryable = backend
            .make_zenoh_queryable(key, |q| {
                async_std::task::spawn(async move {
                    let key = q.key_expr().clone();
                    let val = q.value().unwrap().clone();
                    let _ = q.reply(Ok(Sample::new(key, val))).res_async().await;
                });
            })
            .await;

        PingPong {
            pub_sub: PubSub {
                key: key.to_string(),
                publisher: Box::new(ROS1Client::new(&backend, topic, cycles.clone())),
                subscriber: Box::new(ZenohQueryable {
                    _inner: zenoh_queryable,
                }),
                _discovery_resource: discovery_resource,
            },
            cycles,
        }
    }

    async fn new_ros1_to_zenoh(key: &str, backend: Arc<BridgeChecker>) -> PingPong {
        let cycles = Arc::new(AtomicUsize::new(0));
        let ros1_pub = Arc::new(backend.make_ros_publisher(key));

        let discovery_resource = backend
            .local_resources
            .declare_subscriber(&BridgeChecker::make_topic(key))
            .await
            .unwrap();

        let c = cycles.clone();
        let rpub = ros1_pub.clone();
        let zenoh_sub = backend
            .make_zenoh_subscriber(key, move |msg| {
                let data = msg.value.payload.contiguous().to_vec();
                debug!(
                    "PingPong: transferring {} bytes from Zenoh to ROS1!",
                    data.len()
                );
                rpub.data.send(rosrust::RawMessage(data)).unwrap();
                c.fetch_add(1, Relaxed);
            })
            .await;

        PingPong {
            pub_sub: PubSub {
                key: key.to_string(),
                publisher: Box::new(ROS1Publisher { inner: ros1_pub }),
                subscriber: Box::new(ZenohSubscriber { _inner: zenoh_sub }),
                _discovery_resource: discovery_resource,
            },
            cycles,
        }
    }

    async fn new_zenoh_to_ros1(key: &str, backend: Arc<BridgeChecker>) -> PingPong {
        let cycles = Arc::new(AtomicUsize::new(0));
        let zenoh_pub = Arc::new(backend.make_zenoh_publisher(key).await);

        let discovery_resource = backend
            .local_resources
            .declare_publisher(&BridgeChecker::make_topic(key))
            .await
            .unwrap();

        let c = cycles.clone();
        let zpub = zenoh_pub.clone();
        let ros1_sub = backend.make_ros_subscriber(key, move |msg: rosrust::RawMessage| {
            debug!(
                "PingPong: transferring {} bytes from ROS1 to Zenoh!",
                msg.0.len()
            );
            zpub.put(msg.0).res_sync().unwrap();
            c.fetch_add(1, Relaxed);
        });

        PingPong {
            pub_sub: PubSub {
                key: key.to_string(),
                publisher: Box::new(ZenohPublisher { inner: zenoh_pub }),
                subscriber: Box::new(ROS1Subscriber { inner: ros1_sub }),
                _discovery_resource: discovery_resource,
            },
            cycles,
        }
    }

    async fn start(&self) {
        self.wait_for_pub_sub_ready().await;
        self.start_ping_pong().await;
    }

    async fn start_ping_pong(&self) {
        debug!("Starting ping-pong!");
        let mut data = Vec::new();
        data.reserve(TestParams::data_size() as usize);
        for i in 0..TestParams::data_size() {
            data.push((i % 255) as u8);
        }
        self.pub_sub.publisher.put(data.clone());
    }

    async fn check_pps(&self, pps_measurements: u32) {
        for i in 0..pps_measurements {
            let pps = self.measure_pps().await;
            trace!("PPS #{}: {} \t Key: {}", i, pps, self.pub_sub.key);
            assert!(pps > 0.0);
        }
    }

    async fn measure_pps(&self) -> f64 {
        debug!("Starting measure PPS....");

        let duration_milliseconds = TestParams::pps_measure_period_ms();

        let mut result = 0.0;
        let mut duration: u64 = 0;

        self.cycles.store(0, Relaxed);
        while !(result > 0.0 || duration >= 10000) {
            async_std::task::sleep(core::time::Duration::from_millis(duration_milliseconds)).await;
            duration += duration_milliseconds;
            result += self.cycles.load(Relaxed) as f64;
        }
        debug!("...finished measure PPS!");
        result * 1000.0 / (duration as f64)
    }

    async fn wait_for_pub_sub_ready(&self) {
        assert!(
            Self::wait(
                move || { self.pub_sub.publisher.ready() && self.pub_sub.subscriber.ready() },
                core::time::Duration::from_secs(30)
            )
            .await
        );
        async_std::task::sleep(time::Duration::from_secs(1)).await;
    }

    async fn wait<Waiter>(waiter: Waiter, timeout: core::time::Duration) -> bool
    where
        Waiter: Fn() -> bool,
    {
        let cycles = 1000;
        let micros = timeout.as_micros() / cycles;

        for _i in 0..cycles {
            async_std::task::sleep(core::time::Duration::from_micros(
                micros.try_into().unwrap(),
            ))
            .await;
            if waiter() {
                return true;
            }
        }
        false
    }
}

struct TestEnvironment {
    pub bridge: RunningBridge,
    pub checker: Arc<BridgeChecker>,
    _ros_env: ROSEnvironment,
}
impl TestEnvironment {
    pub fn new() -> TestEnvironment {
        let cfg = IsolatedConfig::default();
        let roscfg = IsolatedROSMaster::default();

        // start environment for ROS
        let ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

        // start bridge
        let bridge = RunningBridge::new(cfg.peer(), roscfg.master_uri());

        // start checker's engine
        let checker = Arc::new(BridgeChecker::new(cfg.peer(), roscfg.master_uri().as_str()));

        // this will wait for the bridge to have some expected initial state and serves two purposes:
        // - asserts on the expected state
        // - performs wait and ensures that everything is properly connected and negotiated within the bridge
        async_std::task::block_on(bridge.assert_ros_ok());
        async_std::task::block_on(bridge.assert_bridge_empy());

        TestEnvironment {
            bridge,
            checker,
            _ros_env: ros_env,
        }
    }

    pub async fn assert_bridge_status_synchronized(&self) {
        self.bridge
            .assert_bridge_status(|| *self.checker.expected_bridge_status.read().unwrap())
            .await;
    }
}

#[derive(PartialEq, Eq, Hash, Display)]
enum Mode {
    Ros1ToZenoh,
    ZenohToRos1,
    Ros1Service,
    Ros1Client,

    FastRun,
}
static UNIQUE_NUMBER: AtomicU64 = AtomicU64::new(0);
async fn ping_pong_duplex_parallel_many_(
    env: &TestEnvironment,
    number: u32,
    mode: std::collections::HashSet<Mode>,
) {
    zenoh_core::zasync_executor_init!();

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

    // create scenarios
    let mut ping_pongs = Vec::new();
    for i in 0..number {
        if mode.contains(&Mode::Ros1ToZenoh) {
            ping_pongs.push(
                PingPong::new_ros1_to_zenoh(
                    make_keyexpr(i, Mode::Ros1ToZenoh).as_str(),
                    env.checker.clone(),
                )
                .await,
            );
        }
        if mode.contains(&Mode::ZenohToRos1) {
            ping_pongs.push(
                PingPong::new_zenoh_to_ros1(
                    make_keyexpr(i, Mode::ZenohToRos1).as_str(),
                    env.checker.clone(),
                )
                .await,
            );
        }
        if mode.contains(&Mode::Ros1Service) {
            ping_pongs.push(
                PingPong::new_ros1_to_zenoh_service(
                    make_keyexpr(i, Mode::Ros1Service).as_str(),
                    env.checker.clone(),
                )
                .await,
            );
        }
        if mode.contains(&Mode::Ros1Client) {
            // allow automatical client bridging because it is disabled by default
            Environment::client_bridging_mode().set(BridgingMode::Auto);

            ping_pongs.push(
                PingPong::new_ros1_to_zenoh_client(
                    make_keyexpr(i, Mode::Ros1Client).as_str(),
                    env.checker.clone(),
                )
                .await,
            );
        }
    }

    // pass scenarios to runners
    let mut vec = Vec::new();
    for ping_pong in ping_pongs {
        if mode.contains(&Mode::FastRun) {
            vec.push(ping_pong.run(1));
        } else {
            vec.push(ping_pong.run(TestParams::pps_measurements()));
        }
    }

    // run
    env.assert_bridge_status_synchronized().await;
    futures::future::join_all(vec).await;
    env.assert_bridge_status_synchronized().await;
}

#[test]
fn ping_pong_zenoh_to_ros1() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::ZenohToRos1]),
    ));
}
#[test]
fn ping_pong_zenoh_to_ros1_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    ));
}

#[test]
fn ping_pong_ros1_to_zenoh() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
}
#[test]
fn ping_pong_ros1_to_zenoh_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
}

#[test]
fn ping_pong_ros1_service() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1Service]),
    ));
}
#[test]
fn ping_pong_ros1_service_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Service]),
    ));
}

#[test]
fn ping_pong_ros1_client() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1Client]),
    ));
}
#[test]
fn ping_pong_ros1_client_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Client]),
    ));
}

#[test]
fn ping_pong_all_sequential() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::ZenohToRos1]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1Service]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([Mode::Ros1Client]),
    ));
}
#[test]
fn ping_pong_all_sequential_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Service]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Client]),
    ));
}

#[test]
fn ping_pong_all_parallel() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        1,
        HashSet::from([
            Mode::ZenohToRos1,
            Mode::Ros1ToZenoh,
            Mode::Ros1Service,
            Mode::Ros1Client,
        ]),
    ));
}

#[test]
fn ping_pong_all_parallel_many() {
    let env = TestEnvironment::new();
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([
            Mode::ZenohToRos1,
            Mode::Ros1ToZenoh,
            Mode::Ros1Service,
            Mode::Ros1Client,
        ]),
    ));
}

async fn main_work(env: &TestEnvironment, main_work_finished: Arc<AtomicBool>) {
    assert!(!main_work_finished.load(Relaxed));
    ping_pong_duplex_parallel_many_(
        env,
        TestParams::many_count(),
        HashSet::from([
            Mode::ZenohToRos1,
            Mode::Ros1ToZenoh,
            Mode::Ros1Service,
            Mode::Ros1Client,
        ]),
    )
    .await;
    main_work_finished.store(true, Relaxed);
}
async fn parallel_subwork(env: &TestEnvironment, main_work_finished: Arc<AtomicBool>) {
    while !main_work_finished.load(Relaxed) {
        ping_pong_duplex_parallel_many_(
            env,
            10,
            HashSet::from([
                Mode::ZenohToRos1,
                Mode::Ros1ToZenoh,
                Mode::Ros1Service,
                Mode::Ros1Client,
                Mode::FastRun,
            ]),
        )
        .await;
    }
}
async fn parallel_subworks(
    env: &TestEnvironment,
    main_work_finished: Arc<AtomicBool>,
    concurrent_subwork_count: u32,
) {
    let mut subworks = Vec::new();
    for _i in 0..concurrent_subwork_count {
        subworks.push(parallel_subwork(env, main_work_finished.clone()));
    }
    futures::future::join_all(subworks).await;
}
#[test]
fn ping_pong_all_overlap_one() {
    let env = TestEnvironment::new();
    let main_work_finished = Arc::new(AtomicBool::new(false));

    let main_work = main_work(&env, main_work_finished.clone());
    let parallel_subworks = parallel_subworks(&env, main_work_finished, 1);
    async_std::task::block_on(futures::future::join(main_work, parallel_subworks));
}
#[test]
fn ping_pong_all_overlap_many() {
    let env = TestEnvironment::new();
    let main_work_finished = Arc::new(AtomicBool::new(false));

    let main_work = main_work(&env, main_work_finished.clone());
    let parallel_subworks = parallel_subworks(&env, main_work_finished, 10);
    async_std::task::block_on(futures::future::join(main_work, parallel_subworks));
}
