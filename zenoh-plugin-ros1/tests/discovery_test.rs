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

use async_std::prelude::FutureExt;
use multiset::HashMultiSet;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use zenoh::{key_expr::keyexpr, prelude::*, session::OpenBuilder, Session};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    discovery::{self, LocalResources, RemoteResources},
    test_helpers::{BridgeChecker, IsolatedConfig},
    topic_descriptor::TopicDescriptor,
};

const TIMEOUT: Duration = Duration::from_secs(60);

fn session_builder(cfg: &IsolatedConfig) -> OpenBuilder<zenoh::config::Config> {
    zenoh::open(cfg.peer())
}

fn remote_resources_builder(session: Arc<Session>) -> discovery::RemoteResourcesBuilder {
    discovery::RemoteResourcesBuilder::new("*".to_string(), "*".to_string(), session)
}

fn make_session(cfg: &IsolatedConfig) -> Arc<Session> {
    session_builder(cfg).wait().unwrap().into_arc()
}

fn make_local_resources(session: Arc<Session>) -> LocalResources {
    LocalResources::new("*".to_owned(), "*".to_owned(), session)
}
fn make_remote_resources(session: Arc<Session>) -> RemoteResources {
    async_std::task::block_on(remote_resources_builder(session).build())
}

#[test]
fn discovery_instantination_one_instance() {
    let session = make_session(&IsolatedConfig::default());
    let _remote = make_remote_resources(session.clone());
    let _local = make_local_resources(session);
}

#[test]
fn discovery_instantination_many_instances() {
    let cfg = IsolatedConfig::default();
    let mut sessions = Vec::new();
    for _i in 0..10 {
        sessions.push(make_session(&cfg));
    }

    let mut discoveries = Vec::new();
    for session in sessions.iter() {
        let remote = make_remote_resources(session.clone());
        let local = make_local_resources(session.clone());
        discoveries.push((remote, local));
    }
}

struct DiscoveryCollector {
    publishers: Arc<Mutex<HashMultiSet<TopicDescriptor>>>,
    subscribers: Arc<Mutex<HashMultiSet<TopicDescriptor>>>,
    services: Arc<Mutex<HashMultiSet<TopicDescriptor>>>,
    clients: Arc<Mutex<HashMultiSet<TopicDescriptor>>>,
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
        builder: discovery::RemoteResourcesBuilder,
    ) -> discovery::RemoteResourcesBuilder {
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
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Publisher => {
                        &p
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Subscriber => {
                        &s
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Service => {
                        &srv
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Client => &cli,
                };
                container.lock().unwrap().insert(topic);
                Box::new(Box::pin(async {}))
            })
            .on_lost(move |b_type, topic| {
                let container = match b_type {
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Publisher => {
                        &p1
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Subscriber => {
                        &s1
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Service => {
                        &srv1
                    }
                    zenoh_plugin_ros1::ros_to_zenoh_bridge::bridge_type::BridgeType::Client => {
                        &cli1
                    }
                };
                container.lock().unwrap().remove(&topic);
                Box::new(Box::pin(async {}))
            })
    }

    pub async fn wait_publishers(&self, expected: HashMultiSet<TopicDescriptor>) {
        Self::wait(&self.publishers, expected).await;
    }

    pub async fn wait_subscribers(&self, expected: HashMultiSet<TopicDescriptor>) {
        Self::wait(&self.subscribers, expected).await;
    }

    pub async fn wait_services(&self, expected: HashMultiSet<TopicDescriptor>) {
        Self::wait(&self.services, expected).await;
    }

    pub async fn wait_clients(&self, expected: HashMultiSet<TopicDescriptor>) {
        Self::wait(&self.clients, expected).await;
    }

    // PRIVATE:
    async fn wait(
        container: &Arc<Mutex<HashMultiSet<TopicDescriptor>>>,
        expected: HashMultiSet<TopicDescriptor>,
    ) {
        while expected != *container.lock().unwrap() {
            async_std::task::sleep(core::time::Duration::from_millis(10)).await;
        }
    }
}

async fn generate_topics(
    count: usize,
    duplication: usize,
    stage: usize,
) -> HashMultiSet<TopicDescriptor> {
    let mut result = HashMultiSet::new();
    for number in 0..count {
        for _dup in 0..duplication {
            unsafe {
                let topic = BridgeChecker::make_topic(keyexpr::from_str_unchecked(
                    format!("name_{}_{}", number, stage).as_str(),
                ));
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

    async fn summarize(
        &self,
    ) -> (
        HashMultiSet<TopicDescriptor>,
        HashMultiSet<TopicDescriptor>,
        HashMultiSet<TopicDescriptor>,
        HashMultiSet<TopicDescriptor>,
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
    local_resources: &LocalResources,
    rcv: &DiscoveryCollector,
    state: &State,
) {
    let (publishers, subscribers, services, clients) = state.summarize().await;

    let mut _pub_entities = Vec::new();
    for publisher in publishers.iter() {
        _pub_entities.push(local_resources.declare_publisher(publisher).await);
    }

    let mut _sub_entities = Vec::new();
    for subscriber in subscribers.iter() {
        _sub_entities.push(local_resources.declare_subscriber(subscriber).await);
    }

    let mut _srv_entities = Vec::new();
    for service in services.iter() {
        _srv_entities.push(local_resources.declare_service(service).await);
    }

    let mut _cl_entities = Vec::new();
    for client in clients.iter() {
        _cl_entities.push(local_resources.declare_client(client).await);
    }

    rcv.wait_publishers(publishers).await;
    rcv.wait_subscribers(subscribers).await;
    rcv.wait_services(services).await;
    rcv.wait_clients(clients).await;
}

async fn run_discovery(scenario: Vec<State>) {
    let cfg = IsolatedConfig::default();

    let src_session = session_builder(&cfg).await.unwrap().into_arc();
    let local_resources = make_local_resources(src_session.clone());

    let rcv = DiscoveryCollector::new();
    let rcv_session = session_builder(&cfg).await.unwrap().into_arc();
    let _rcv_discovery = rcv
        .use_builder(remote_resources_builder(rcv_session))
        .build()
        .await;

    for scene in scenario {
        test_state_transition(&local_resources, &rcv, &scene)
            .timeout(TIMEOUT)
            .await
            .expect("Timeout waiting state transition!");
    }
}

#[test]
fn discover_single_publisher() {
    async_std::task::block_on(run_discovery(
        [State::default().publishers(1, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_single_subscriber() {
    async_std::task::block_on(run_discovery(
        [State::default().subscribers(1, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_single_service() {
    async_std::task::block_on(run_discovery(
        [State::default().services(1, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_single_client() {
    async_std::task::block_on(run_discovery(
        [State::default().clients(1, 1)].into_iter().collect(),
    ));
}
#[test]
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
fn discover_multiple_publishers() {
    async_std::task::block_on(run_discovery(
        [State::default().publishers(100, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_multiple_subscribers() {
    async_std::task::block_on(run_discovery(
        [State::default().subscribers(100, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_multiple_services() {
    async_std::task::block_on(run_discovery(
        [State::default().services(100, 1)].into_iter().collect(),
    ));
}
#[test]
fn discover_multiple_clients() {
    async_std::task::block_on(run_discovery(
        [State::default().clients(100, 1)].into_iter().collect(),
    ));
}
#[test]
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
