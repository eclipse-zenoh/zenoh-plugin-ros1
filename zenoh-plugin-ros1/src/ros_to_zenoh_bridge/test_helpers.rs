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

use log::error;
use rosrust::{Client, RawMessage, RawMessageDescription};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::*;
use std::sync::{Arc, Mutex, RwLock};
use std::{net::SocketAddr, str::FromStr, sync::atomic::AtomicU16};
use zenoh::config::ModeDependentValue;
use zenoh::prelude::OwnedKeyExpr;
use zenoh::prelude::SplitBuffer;
use zenoh::sample::Sample;
use zenoh_core::{bail, zlock, zresult::ZResult, AsyncResolve, SyncResolve};

use super::discovery::LocalResources;
use super::ros1_to_zenoh_bridge_impl::{work_cycle, BridgeStatus, RosStatus};
use super::topic_descriptor::TopicDescriptor;
use super::topic_utilities;
use super::topic_utilities::make_topic_key;
use super::{ros1_client, zenoh_client};

pub struct IsolatedPort {
    pub port: u16,
}
impl Default for IsolatedPort {
    fn default() -> Self {
        static TEST_PORT: AtomicU16 = AtomicU16::new(20000);
        Self {
            port: TEST_PORT.fetch_add(1, SeqCst),
        }
    }
}

#[derive(Default)]
pub struct IsolatedConfig {
    port: IsolatedPort,
}
impl IsolatedConfig {
    pub fn peer(&self) -> zenoh::config::Config {
        let mut config = zenoh::config::peer();
        config
            .scouting
            .multicast
            .set_address(Some(
                SocketAddr::from_str(format!("224.0.0.224:{}", self.port.port).as_str()).unwrap(),
            ))
            .unwrap();
        config
            .timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        config
    }
}

#[derive(Default)]
pub struct IsolatedROSMaster {
    pub port: IsolatedPort,
}
impl IsolatedROSMaster {
    pub fn master_uri(&self) -> String {
        format!("http://localhost:{}/", self.port.port)
    }
}

pub async fn wait<Waiter>(waiter: Waiter, timeout: core::time::Duration) -> bool
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

pub struct RunningBridge {
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
                let mut val = zlock!(ros_status);
                *val = v;
            },
            move |status| {
                let mut my_status = zlock!(bridge_status);
                *my_status = status;
            },
        )
        .await
        .unwrap();
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
                let expected = (status)();
                let real = self.bridge_status.lock().unwrap();
                *real == expected
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

pub struct ROSEnvironment {
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

pub struct BridgeChecker {
    ros_client: ros1_client::Ros1Client,
    zenoh_client: zenoh_client::ZenohClient,
    pub local_resources: LocalResources,

    pub expected_bridge_status: Arc<RwLock<BridgeStatus>>,
}
impl BridgeChecker {
    // PUBLIC
    pub fn new(config: zenoh::config::Config, ros_master_uri: &str) -> BridgeChecker {
        let session = zenoh::open(config).res_sync().unwrap().into_arc();
        BridgeChecker {
            ros_client: ros1_client::Ros1Client::new("test_ros_node", ros_master_uri).unwrap(),
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

    pub fn make_topic(name: &str) -> TopicDescriptor {
        topic_utilities::make_topic("some/testdatatype", "anymd5", name.try_into().unwrap())
    }

    pub fn make_zenoh_key(topic: &TopicDescriptor) -> OwnedKeyExpr {
        topic_utilities::make_zenoh_key(topic)
    }
}

pub struct RAIICounter<T>
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

pub struct TestParams;
impl TestParams {
    pub fn many_count() -> u32 {
        Self::env_var("TEST_ROS1_TO_ZENOH_MANY_COUNT", 10)
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

pub struct ZenohPublisher {
    pub inner: Arc<zenoh::publication::Publisher<'static>>,
}
pub struct ROS1Publisher {
    pub inner: Arc<RAIICounter<rosrust::Publisher<rosrust::RawMessage>>>,
}
pub struct ZenohQuery {
    inner: Arc<BridgeChecker>,
    key: String,
    running: Arc<AtomicBool>,
    cycles: Arc<AtomicUsize>,
}
impl ZenohQuery {
    pub fn new(inner: Arc<BridgeChecker>, key: String, cycles: Arc<AtomicUsize>) -> Self {
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

pub struct ROS1Client {
    topic: TopicDescriptor,
    running: Arc<AtomicBool>,
    cycles: Arc<AtomicUsize>,
    ros1_client: Arc<RAIICounter<Client<RawMessage>>>,
}
impl ROS1Client {
    pub fn new(inner: &BridgeChecker, topic: TopicDescriptor, cycles: Arc<AtomicUsize>) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let ros1_client = Arc::new(inner.make_ros_client(make_topic_key(&topic)));
        Self {
            topic,
            running,
            cycles,
            ros1_client,
        }
    }

    fn query_loop(
        description: RawMessageDescription,
        running: Arc<AtomicBool>,
        data: Vec<u8>,
        cycles: Arc<AtomicUsize>,
        ros1_client: Arc<RAIICounter<Client<RawMessage>>>,
    ) {
        while running.load(Relaxed) {
            match Self::make_query(description.clone(), &data, &ros1_client) {
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
        description: RawMessageDescription,
        data: &Vec<u8>,
        ros1_client: &Arc<RAIICounter<Client<RawMessage>>>,
    ) -> ZResult<()> {
        match ros1_client
            .data
            .req_with_description(&RawMessage(data.clone()), description)
        {
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

pub trait Publisher {
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
        let description = RawMessageDescription {
            msg_definition: String::from("*"),
            md5sum: self.topic.md5.clone(),
            msg_type: self.topic.datatype.clone(),
        };

        async_std::task::spawn_blocking(|| {
            Self::query_loop(description, running, data, cycles, ros1_client)
        });
    }

    fn ready(&self) -> bool {
        let description = RawMessageDescription {
            msg_definition: String::from("*"),
            md5sum: self.topic.md5.clone(),
            msg_type: self.topic.datatype.clone(),
        };
        let data = (0..10).collect();
        Self::make_query(description, &data, &self.ros1_client).is_ok()
    }
}

pub struct ZenohSubscriber {
    pub _inner: zenoh::subscriber::Subscriber<'static, ()>,
}
pub struct ZenohQueryable {
    pub _inner: zenoh::queryable::Queryable<'static, ()>,
}
pub struct ROS1Subscriber {
    pub inner: RAIICounter<rosrust::Subscriber>,
}
pub struct ROS1Service {
    pub _inner: RAIICounter<rosrust::Service>,
}

pub trait Subscriber {
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
