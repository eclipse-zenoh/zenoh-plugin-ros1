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
use zenoh_core::{bail, zresult::ZResult, SyncResolve};

use std::{
    collections::HashSet,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering::*},
        Arc, Mutex, RwLock,
    },
};

use zplugin_ros1::ros_to_zenoh_bridge::ros1_to_zenoh_bridge_impl::{
    work_cycle, BridgeStatus, RosStatus,
};
use zplugin_ros1::ros_to_zenoh_bridge::{
    discovery::{LocalResource, LocalResources},
    test_helpers::{IsolatedConfig, IsolatedROSMaster},
};
use zplugin_ros1::ros_to_zenoh_bridge::{ros1_client, zenoh_client};

use log::{debug, error, trace};
use rosrust::{Client, RawMessage};
use std::sync::atomic::AtomicUsize;
use zenoh::prelude::r#async::*;

use std::process::Command;
use std::{thread, time};

async fn wait<Waiter>(waiter: Waiter, timeout: core::time::Duration) -> bool
where
    Waiter: Fn() -> bool,
{
    let cycles = 1000;
    let millis = timeout.as_millis() / cycles + 1;

    for _i in 0..cycles {
        async_std::task::sleep(core::time::Duration::from_millis(
            millis.try_into().unwrap(),
        ))
        .await;
        if waiter() {
            return true;
        }
    }
    false
}

struct RunningBridge {
    flag: Arc<AtomicBool>,

    ros_status: Arc<Mutex<RosStatus>>,

    bridge_status: Arc<Mutex<BridgeStatus>>,
}
impl RunningBridge {
    pub fn new(config: zenoh::config::Config, ros_master_uri: String) -> RunningBridge {
        let result = RunningBridge {
            flag: Arc::new(AtomicBool::new(true)),
            ros_status: Arc::new(Mutex::new(RosStatus::Unknown)),
            bridge_status: Arc::new(Mutex::new(BridgeStatus::default())),
        };
        async_std::task::spawn(Self::run(
            ros_master_uri,
            config,
            result.flag.clone(),
            result.ros_status.clone(),
            result.bridge_status.clone(),
        ));
        result
    }

    async fn run(
        ros_master_uri: String,
        config: zenoh::config::Config,
        flag: Arc<AtomicBool>,
        ros_status: Arc<Mutex<RosStatus>>,
        bridge_status: Arc<Mutex<BridgeStatus>>,
    ) {
        let session = zenoh::open(config).res_async().await.unwrap().into_arc();
        work_cycle(
            ros_master_uri.as_str(),
            session,
            flag,
            move |v| {
                let mut val = ros_status.lock().unwrap();
                *val = v;
            },
            move |status| {
                let mut my_status = bridge_status.lock().unwrap();
                *my_status = status;
            },
        )
        .await;
    }

    pub async fn assert_ros_error(&self) {
        self.assert_status(RosStatus::Error).await;
    }
    pub async fn assert_ros_ok(&self) {
        self.assert_status(RosStatus::Ok).await;
    }
    pub async fn assert_status(&self, status: RosStatus) {
        assert!(
            self.wait_ros_status(status, core::time::Duration::from_secs(10))
                .await
        );
    }
    pub async fn wait_ros_status(&self, status: RosStatus, timeout: core::time::Duration) -> bool {
        wait(
            move || {
                let val = self.ros_status.lock().unwrap();
                *val == status
            },
            timeout,
        )
        .await
    }

    pub async fn assert_bridge_empy(&self) {
        self.assert_bridge_status(BridgeStatus::default).await;
    }
    pub async fn assert_bridge_status<F: Fn() -> BridgeStatus>(&self, status: F) {
        assert!(
            self.wait_bridge_status(status, core::time::Duration::from_secs(120))
                .await
        );
    }
    pub async fn wait_bridge_status<F: Fn() -> BridgeStatus>(
        &self,
        status: F,
        timeout: core::time::Duration,
    ) -> bool {
        wait(
            move || {
                let val = self.bridge_status.lock().unwrap();
                *val == (status)()
            },
            timeout,
        )
        .await
    }
}
impl Drop for RunningBridge {
    fn drop(&mut self) {
        self.flag.store(false, Relaxed);
    }
}

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

struct ZenohPublisher {
    inner: Arc<zenoh::publication::Publisher<'static>>,
}
struct ROS1Publisher {
    inner: Arc<RAIICounter<rosrust::Publisher<rosrust::RawMessage>>>,
}
struct ZenohQuery {
    inner: Arc<BridgeChecker>,
    key: String,
    running: Arc<AtomicBool>,
    cycles: Arc<AtomicUsize>,
}
impl ZenohQuery {
    fn new(inner: Arc<BridgeChecker>, key: String, cycles: Arc<AtomicUsize>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        Self {
            inner,
            key,
            running,
            cycles,
        }
    }

    async fn make_query(inner: &Arc<BridgeChecker>, key: &str, data: &Vec<u8>) -> ZResult<()> {
        let query = inner.make_zenoh_query_sync(key, data.clone()).await;
        match query.recv_async().await {
            Ok(reply) => match reply.sample {
                Ok(value) => {
                    let returned_data = value.payload.contiguous().to_vec();
                    if data.eq(&returned_data) {
                        Ok(())
                    } else {
                        bail!("ZenohQuery: data is not equal! \n Sent data: {:?} \nReturned data: {:?}", data, returned_data);
                    }
                }
                Err(e) => {
                    bail!("ZenohQuery: got reply with error: {}", e);
                }
            },
            Err(e) => {
                bail!("ZenohQuery: failed to get reply with error: {}", e);
            }
        }
    }

    async fn query_loop(
        inner: Arc<BridgeChecker>,
        key: String,
        running: Arc<AtomicBool>,
        data: Vec<u8>,
        cycles: Arc<AtomicUsize>,
    ) {
        while running.load(Relaxed) {
            match Self::make_query(&inner, &key, &data).await {
                Ok(_) => {
                    cycles.fetch_add(1, SeqCst);
                }
                Err(e) => {
                    error!("{}", e);
                }
            }
        }
    }
}
impl Drop for ZenohQuery {
    fn drop(&mut self) {
        self.running.store(false, Relaxed);
    }
}

struct ROS1Client {
    running: Arc<AtomicBool>,
    cycles: Arc<AtomicUsize>,
    ros1_client: Arc<RAIICounter<Client<RawMessage>>>,
}
impl ROS1Client {
    fn new(inner: Arc<BridgeChecker>, key: String, cycles: Arc<AtomicUsize>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let ros1_client = Arc::new(inner.make_ros_client(&key));
        Self {
            running,
            cycles,
            ros1_client,
        }
    }

    fn query_loop(
        running: Arc<AtomicBool>,
        data: Vec<u8>,
        cycles: Arc<AtomicUsize>,
        ros1_client: Arc<RAIICounter<Client<RawMessage>>>,
    ) {
        while running.load(Relaxed) {
            match Self::make_query(&data, &ros1_client) {
                Ok(_) => {
                    cycles.fetch_add(1, SeqCst);
                }
                Err(e) => {
                    error!("{}", e);
                }
            }
        }
    }

    fn make_query(
        data: &Vec<u8>,
        ros1_client: &Arc<RAIICounter<Client<RawMessage>>>,
    ) -> ZResult<()> {
        match ros1_client.data.req(&RawMessage(data.clone())) {
            Ok(reply) => match reply {
                Ok(msg) => {
                    if data.eq(&msg.0) {
                        Ok(())
                    } else {
                        bail!("ROS1Client: data is not equal! \n Sent data: {:?} \nReturned data: {:?}", data, msg.0);
                    }
                }
                Err(e) => {
                    bail!("ROS1Client: got reply with error: {}", e);
                }
            },
            Err(e) => {
                bail!("ROS1Client: failed to send request with error: {}", e);
            }
        }
    }
}
impl Drop for ROS1Client {
    fn drop(&mut self) {
        self.running.store(false, Relaxed);
    }
}

trait Publisher {
    fn put(&self, data: Vec<u8>);
    fn ready(&self) -> bool {
        true
    }
}
impl Publisher for ZenohPublisher {
    fn put(&self, data: Vec<u8>) {
        let inner = self.inner.clone();
        async_std::task::spawn_blocking(move || inner.put(data).res_sync().unwrap());
    }
}
impl Publisher for ROS1Publisher {
    fn put(&self, data: Vec<u8>) {
        let inner = self.inner.clone();
        async_std::task::spawn_blocking(move || {
            inner.data.send(rosrust::RawMessage(data)).unwrap()
        });
    }

    fn ready(&self) -> bool {
        self.inner.data.subscriber_count() != 0
    }
}
impl Publisher for ZenohQuery {
    fn put(&self, data: Vec<u8>) {
        async_std::task::spawn(Self::query_loop(
            self.inner.clone(),
            self.key.clone(),
            self.running.clone(),
            data,
            self.cycles.clone(),
        ));
    }

    fn ready(&self) -> bool {
        let data = (0..10).collect();
        async_std::task::block_on(
            async move { Self::make_query(&self.inner, &self.key, &data).await },
        )
        .is_ok()
    }
}
impl Publisher for ROS1Client {
    fn put(&self, data: Vec<u8>) {
        let running = self.running.clone();
        let cycles = self.cycles.clone();
        let ros1_client = self.ros1_client.clone();

        async_std::task::spawn_blocking(|| Self::query_loop(running, data, cycles, ros1_client));
    }

    fn ready(&self) -> bool {
        let data = (0..10).collect();
        Self::make_query(&data, &self.ros1_client).is_ok()
    }
}

struct ZenohSubscriber {
    _inner: zenoh::subscriber::Subscriber<'static, ()>,
}
struct ZenohQueryable {
    _inner: zenoh::queryable::Queryable<'static, ()>,
}
struct ROS1Subscriber {
    inner: RAIICounter<rosrust::Subscriber>,
}
struct ROS1Service {
    _inner: RAIICounter<rosrust::Service>,
}

trait Subscriber {
    fn ready(&self) -> bool {
        true
    }
}
impl Subscriber for ZenohSubscriber {}
impl Subscriber for ZenohQueryable {}
impl Subscriber for ROS1Subscriber {
    fn ready(&self) -> bool {
        self.inner.data.publisher_count() > 0 && !self.inner.data.publisher_uris().is_empty()
    }
}
impl Subscriber for ROS1Service {}

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
            .await;
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

        let discovery_resource = backend
            .local_resources
            .declare_service(&BridgeChecker::make_topic(key))
            .await;
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
                publisher: Box::new(ROS1Client::new(backend, key.to_string(), cycles.clone())),
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
            .await;

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
            .await;

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
        data.reserve(TestEnvironment::data_size() as usize);
        for i in 0..TestEnvironment::data_size() {
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

        let duration_milliseconds = TestEnvironment::pps_measure_period_ms();

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

struct ROSEnvironment {
    ros_master_port: u16,
    rosmaster: Option<std::process::Child>,
}
impl Drop for ROSEnvironment {
    fn drop(&mut self) {
        if let Some(mut child) = self.rosmaster.take() {
            if child.kill().is_ok() {
                let _ = child.wait();
            }
        }
    }
}
impl ROSEnvironment {
    pub fn new(ros_master_port: u16) -> Self {
        ROSEnvironment {
            rosmaster: None,
            ros_master_port,
        }
    }

    pub fn with_master(mut self) -> Self {
        assert!(self.rosmaster.is_none());

        let rosmaster_cmd = Command::new("rosmaster")
            .arg(format!("-p {}", self.ros_master_port).as_str())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        match rosmaster_cmd {
            Ok(val) => {
                self.rosmaster = Some(val);
            }
            Err(e) => {
                println!("Error while starting rosmaster: {}", e);
                panic!("{}", e);
            }
        }

        self
    }

    pub fn without_master(mut self) -> Self {
        assert!(self.rosmaster.is_some());
        let mut child = self.rosmaster.take().unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        self
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

    pub fn many_count() -> u32 {
        Self::env_var("TEST_ROS1_TO_ZENOH_MANY_COUNT", 40)
    }

    pub fn pps_measurements() -> u32 {
        Self::env_var("TEST_ROS1_TO_ZENOH_PPS_ITERATIONS", 100)
    }

    pub fn pps_measure_period_ms() -> u64 {
        Self::env_var("TEST_ROS1_TO_ZENOH_PPS_PERIOD_MS", 1)
    }

    pub fn data_size() -> u32 {
        Self::env_var("TEST_ROS1_TO_ZENOH_DATA_SIZE", 16)
    }

    pub async fn assert_bridge_status_synchronized(&self) {
        self.bridge
            .assert_bridge_status(|| *self.checker.expected_bridge_status.read().unwrap())
            .await;
    }

    // PRIVATE
    fn env_var<Tvar>(key: &str, default: Tvar) -> Tvar
    where
        Tvar: FromStr,
    {
        if let Ok(val) = std::env::var(key) {
            if let Ok(val) = val.parse::<Tvar>() {
                return val;
            }
        }
        default
    }
}

struct RAIICounter<T>
where
    T: Sized,
{
    pub data: T,
    on_destroy: Box<dyn Fn() + Sync + Send + 'static>,
}

impl<T> RAIICounter<T>
where
    T: Sized,
{
    fn new<F>(data: T, on_destroy: F) -> Self
    where
        F: Fn() + Sync + Send + 'static,
    {
        Self {
            data,
            on_destroy: Box::new(on_destroy),
        }
    }
}

impl<T> Drop for RAIICounter<T>
where
    T: Sized,
{
    fn drop(&mut self) {
        (self.on_destroy)();
    }
}

struct BridgeChecker {
    ros_client: ros1_client::Ros1Client,
    zenoh_client: zenoh_client::ZenohClient,
    local_resources: LocalResources,

    pub expected_bridge_status: Arc<RwLock<BridgeStatus>>,
}
impl BridgeChecker {
    // PUBLIC
    pub fn new(config: zenoh::config::Config, ros_master_uri: &str) -> BridgeChecker {
        let session = zenoh::open(config).res_sync().unwrap().into_arc();
        BridgeChecker {
            ros_client: ros1_client::Ros1Client::new("test_ros_node", ros_master_uri),
            zenoh_client: zenoh_client::ZenohClient::new(session.clone()),
            local_resources: LocalResources::new("*".to_string(), "*".to_string(), session),
            expected_bridge_status: Arc::new(RwLock::new(BridgeStatus::default())),
        }
    }

    pub async fn make_zenoh_subscriber<C>(
        &self,
        name: &str,
        callback: C,
    ) -> zenoh::subscriber::Subscriber<'static, ()>
    where
        C: Fn(Sample) + Send + Sync + 'static,
    {
        self.zenoh_client
            .subscribe(Self::make_zenoh_key(&Self::make_topic(name)), callback)
            .await
            .unwrap()
    }

    pub async fn make_zenoh_publisher(&self, name: &str) -> zenoh::publication::Publisher<'static> {
        self.zenoh_client
            .publish(Self::make_zenoh_key(&Self::make_topic(name)))
            .await
            .unwrap()
    }

    pub async fn make_zenoh_queryable<Callback>(
        &self,
        name: &str,
        callback: Callback,
    ) -> zenoh::queryable::Queryable<'static, ()>
    where
        Callback: Fn(zenoh::queryable::Query) + Send + Sync + 'static,
    {
        self.zenoh_client
            .make_queryable(Self::make_zenoh_key(&Self::make_topic(name)), callback)
            .await
            .unwrap()
    }

    pub async fn make_zenoh_query_sync(
        &self,
        name: &str,
        data: Vec<u8>,
    ) -> flume::Receiver<zenoh::query::Reply> {
        self.zenoh_client
            .make_query_sync(Self::make_zenoh_key(&Self::make_topic(name)), data)
            .await
            .unwrap()
    }

    pub fn make_ros_publisher(&self, name: &str) -> RAIICounter<rosrust::Publisher<RawMessage>> {
        let status = self.expected_bridge_status.clone();
        status.write().unwrap().ros_publishers.0 += 1;
        status.write().unwrap().ros_publishers.1 += 1;
        RAIICounter::new(
            self.ros_client.publish(&Self::make_topic(name)).unwrap(),
            move || {
                if let Ok(mut locked) = status.write() {
                    locked.ros_publishers.0 -= 1;
                    locked.ros_publishers.1 -= 1;
                }
            },
        )
    }

    pub fn make_ros_subscriber<T, F>(
        &self,
        name: &str,
        callback: F,
    ) -> RAIICounter<rosrust::Subscriber>
    where
        T: rosrust::Message,
        F: Fn(T) + Send + 'static,
    {
        let status = self.expected_bridge_status.clone();
        status.write().unwrap().ros_subscribers.0 += 1;
        status.write().unwrap().ros_subscribers.1 += 1;
        RAIICounter::new(
            self.ros_client
                .subscribe(&Self::make_topic(name), callback)
                .unwrap(),
            move || {
                if let Ok(mut locked) = status.write() {
                    locked.ros_subscribers.0 -= 1;
                    locked.ros_subscribers.1 -= 1;
                }
            },
        )
    }

    pub fn make_ros_client(&self, name: &str) -> RAIICounter<rosrust::Client<rosrust::RawMessage>> {
        let status = self.expected_bridge_status.clone();
        status.write().unwrap().ros_clients.0 += 1;
        status.write().unwrap().ros_clients.1 += 1;
        RAIICounter::new(
            self.ros_client.client(&Self::make_topic(name)).unwrap(),
            move || {
                if let Ok(mut locked) = status.write() {
                    locked.ros_clients.0 -= 1;
                    locked.ros_clients.1 -= 1;
                }
            },
        )
    }

    pub fn make_ros_service<F>(&self, name: &str, handler: F) -> RAIICounter<rosrust::Service>
    where
        F: Fn(rosrust::RawMessage) -> rosrust::ServiceResult<rosrust::RawMessage>
            + Send
            + Sync
            + 'static,
    {
        let status = self.expected_bridge_status.clone();
        status.write().unwrap().ros_services.0 += 1;
        status.write().unwrap().ros_services.1 += 1;
        RAIICounter::new(
            self.ros_client
                .service::<rosrust::RawMessage, F>(&Self::make_topic(name), handler)
                .unwrap(),
            move || {
                if let Ok(mut locked) = status.write() {
                    locked.ros_services.0 -= 1;
                    locked.ros_services.1 -= 1;
                }
            },
        )
    }

    // PRIVATE
    fn make_topic(name: &str) -> rosrust::api::Topic {
        rosrust::api::Topic {
            name: name.to_string(),
            datatype: "some_datatype".to_string(),
        }
    }

    fn make_zenoh_key(topic: &rosrust::api::Topic) -> &str {
        return topic.name.trim_start_matches('/').trim_end_matches('/');
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

    let make_keyexpr = |i: u32, mode: Mode| -> String {
        format!(
            "/some/key/expr{}_{}_{}",
            i,
            mode,
            UNIQUE_NUMBER.fetch_add(1, SeqCst)
        )
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
            vec.push(ping_pong.run(TestEnvironment::pps_measurements()));
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
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
        HashSet::from([Mode::ZenohToRos1]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestEnvironment::many_count(),
        HashSet::from([Mode::Ros1ToZenoh]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestEnvironment::many_count(),
        HashSet::from([Mode::Ros1Service]),
    ));
    futures::executor::block_on(ping_pong_duplex_parallel_many_(
        &env,
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
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
        TestEnvironment::many_count(),
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
