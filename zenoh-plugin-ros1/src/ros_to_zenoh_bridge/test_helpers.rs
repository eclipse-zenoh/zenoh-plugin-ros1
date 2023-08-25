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

use rosrust::RawMessage;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::*;
use std::sync::{Arc, Mutex, RwLock};
use std::{net::SocketAddr, str::FromStr, sync::atomic::AtomicU16};
use zenoh::config::ModeDependentValue;
use zenoh::sample::Sample;
use zenoh_core::{AsyncResolve, SyncResolve};

use super::discovery::LocalResources;
use super::ros1_to_zenoh_bridge_impl::{work_cycle, BridgeStatus, RosStatus};
use super::topic_utilities;
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

    pub fn make_topic(name: &str) -> rosrust::api::Topic {
        topic_utilities::make_topic("some/very.complicated/datatype//", name.try_into().unwrap())
    }

    pub fn make_zenoh_key(topic: &rosrust::api::Topic) -> &str {
        return topic.name.trim_start_matches('/').trim_end_matches('/');
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
