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
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::*},
        Arc,
    },
    thread, time,
    time::Duration,
};

use strum_macros::Display;
use tracing::{debug, trace};
use zenoh::{key_expr::KeyExpr, Wait};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    bridging_mode::BridgingMode,
    discovery::LocalResource,
    environment::Environment,
    test_helpers::{
        self, wait_async_fn, BridgeChecker, IsolatedConfig, IsolatedROSMaster, Publisher,
        ROS1Client, ROS1Publisher, ROS1Service, ROS1Subscriber, ROSEnvironment, RunningBridge,
        Subscriber, TestParams, ZenohPublisher, ZenohQuery, ZenohQueryable, ZenohSubscriber,
    },
};

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_no_master_init_and_exit_immed() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_no_master_init_and_wait() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_with_master_init_and_exit_immed() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_with_master_init_and_wait() {
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());

    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_with_master_init_and_loose_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut _ros_env = Some(ROSEnvironment::new(roscfg.port.port).with_master());
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
    _ros_env = None;
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_with_master_init_and_wait_for_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut _ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_error().await;
    bridge.assert_bridge_empy().await;
    _ros_env = _ros_env.with_master();
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
    thread::sleep(time::Duration::from_secs(1));
    bridge.assert_ros_ok().await;
    bridge.assert_bridge_empy().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn env_checks_with_master_init_and_reconnect_many_times_to_master() {
    let roscfg = IsolatedROSMaster::default();
    let mut ros_env = ROSEnvironment::new(roscfg.port.port);
    let bridge = RunningBridge::new(IsolatedConfig::default().peer(), roscfg.master_uri());
    for _i in 0..20 {
        bridge.assert_ros_error().await;
        bridge.assert_bridge_empy().await;
        ros_env = ros_env.with_master();
        bridge.assert_ros_ok().await;
        bridge.assert_bridge_empy().await;
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
                tokio::spawn(async move {
                    let key = q.key_expr().clone();
                    let val = q.payload().unwrap().clone();
                    let _ = q.reply(key, val).await;
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
                c.fetch_add(1, Relaxed);
                let data = msg.payload().to_bytes().to_vec();
                debug!(
                    "PingPong: transferring {} bytes from Zenoh to ROS1!",
                    data.len()
                );
                rpub.data.send(rosrust::RawMessage(data)).unwrap();
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
            c.fetch_add(1, Relaxed);
            debug!(
                "PingPong: transferring {} bytes from ROS1 to Zenoh!",
                msg.0.len()
            );
            zpub.put(msg.0).wait().unwrap();
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
        assert!(self.start_ping_pong().await);
    }

    async fn start_ping_pong(&self) -> bool {
        debug!("Starting ping-pong!");
        let mut data = Vec::with_capacity(TestParams::data_size() as usize);
        for i in 0..TestParams::data_size() {
            data.push((i % 255) as u8);
        }

        tokio::time::timeout(Duration::from_secs(30), async {
            while {
                self.pub_sub.publisher.put(data.clone());
                !wait_async_fn(|| self.cycles.load(Relaxed) > 0, Duration::from_secs(5)).await
            } {
                debug!("Restarting ping-pong!");
            }
        })
        .await
        .is_ok()
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
        while !(result > 0.0 || duration >= 30000) {
            tokio::time::sleep(Duration::from_millis(duration_milliseconds)).await;
            duration += duration_milliseconds;
            result += self.cycles.load(Relaxed) as f64;
        }
        debug!("...finished measure PPS!");
        result * 1000.0 / (duration as f64)
    }

    async fn wait_for_pub_sub_ready(&self) {
        assert!(
            test_helpers::wait_async(
                || async {
                    self.pub_sub.publisher.ready().await && self.pub_sub.subscriber.ready()
                },
                Duration::from_secs(30)
            )
            .await
        );
    }
}

struct TestEnvironment {
    pub bridge: RunningBridge,
    pub checker: Arc<BridgeChecker>,
    _ros_env: ROSEnvironment,
}
impl TestEnvironment {
    pub async fn new() -> TestEnvironment {
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
        bridge.assert_ros_ok().await;
        bridge.assert_bridge_empy().await;
        checker.assert_zenoh_peers(1).await;

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
    zenoh::internal::zasync_executor_init!();

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

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_zenoh_to_ros1() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::ZenohToRos1])).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_zenoh_to_ros1_many() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_to_zenoh() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1ToZenoh])).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_to_zenoh_many() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_service() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1Service])).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_service_many() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Service]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_client() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1Client])).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_ros1_client_many() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Client]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_sequential() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::ZenohToRos1])).await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1ToZenoh])).await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1Service])).await;
    ping_pong_duplex_parallel_many_(&env, 1, HashSet::from([Mode::Ros1Client])).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_sequential_many() {
    let env = TestEnvironment::new().await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    )
    .await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    )
    .await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Service]),
    )
    .await;
    ping_pong_duplex_parallel_many_(
        &env,
        TestParams::many_count(),
        HashSet::from([Mode::Ros1Client]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_parallel() {
    let env = TestEnvironment::new().await;
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

#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_parallel_many() {
    let env = TestEnvironment::new().await;
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
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_overlap_one() {
    let env = TestEnvironment::new().await;
    let main_work_finished = Arc::new(AtomicBool::new(false));

    let main_work = main_work(&env, main_work_finished.clone());
    let parallel_subworks = parallel_subworks(&env, main_work_finished, 1);
    futures::future::join(main_work, parallel_subworks).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn ping_pong_all_overlap_many() {
    let env = TestEnvironment::new().await;
    let main_work_finished = Arc::new(AtomicBool::new(false));

    let main_work = main_work(&env, main_work_finished.clone());
    let parallel_subworks = parallel_subworks(&env, main_work_finished, 10);
    futures::future::join(main_work, parallel_subworks).await;
}
