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

use futures::Future;
use log::error;
use rosrust;

use std::sync::Arc;
use std::time::Duration;

use zenoh::prelude::r#async::*;

use std::str;

use super::aloha_declaration::AlohaDeclaration;
use super::aloha_subscription::{AlohaSubscription, AlohaSubscriptionBuilder};
use super::bridge_type::BridgeType;
use super::topic_utilities::{make_topic, make_zenoh_key};

zenoh::kedefine!(
    pub discovery_format: "ros1_discovery_info/${discovery_namespace:*}/${resource_class:*}/${data_type:*}/${bridge_namespace:*}/${topic:**}",
);
// example:
// ros1_discovery_info/discovery_namespace/publishers|subscribers|services|clients/data_type/bridge_namespace/some/ros/topic
// where
// discovery_namespace - namespace to isolate different discovery pools. Would be * by default ( == global namespace)
// bridge_namespace - namespace to prefix bridge's resources. Would be * by default ( == global namespace)
// publishers|subscribers|services|clients - one of

const ROS1_DISCOVERY_INFO_PUBLISHERS_CLASS: &str = "pub";
const ROS1_DISCOVERY_INFO_SUBSCRIBERS_CLASS: &str = "sub";
const ROS1_DISCOVERY_INFO_SERVICES_CLASS: &str = "srv";
const ROS1_DISCOVERY_INFO_CLIENTS_CLASS: &str = "cl";

struct RemoteResources {
    _subscriber: Option<AlohaSubscription>,
}
impl RemoteResources {
    async fn new<F>(
        session: Arc<zenoh::Session>,
        discovery_namespace: String,
        on_discovered: F,
        on_lost: F,
    ) -> Self
    where
        F: Fn(
                BridgeType,
                rosrust::api::Topic,
            ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
            + Send
            + Sync
            + 'static,
    {
        let subscriber: Option<AlohaSubscription>;

        // make proper discovery keyexpr
        let mut formatter = discovery_format::formatter();
        let discovery_keyexpr = zenoh::keformat!(
            formatter,
            discovery_namespace = discovery_namespace,
            resource_class = "*",
            data_type = "*",
            bridge_namespace = "*",
            topic = "*/**"
        )
        .unwrap();

        let _on_discovered = Arc::new(on_discovered);
        let _on_lost = Arc::new(on_lost);

        let subscription = AlohaSubscriptionBuilder::new(
            session,
            discovery_keyexpr.clone(),
            Duration::from_secs(1),
        )
        .on_resource_declared(move |key| {
            Box::new(Box::pin(Self::process(
                key.into_owned(),
                _on_discovered.clone(),
            )))
        })
        .on_resource_undeclared(move |key| {
            Box::new(Box::pin(Self::process(key.into_owned(), _on_lost.clone())))
        })
        .build()
        .await;

        match subscription {
            Ok(s) => {
                subscriber = Some(s);
            }
            Err(e) => {
                error!("ROS1 Discovery: error creating querying subscriber: {}", e);
                subscriber = None;
            }
        }

        Self {
            _subscriber: subscriber,
        }
    }

    // PRIVATE:
    async fn process<F>(data: KeyExpr<'_>, callback: Arc<F>)
    where
        F: Fn(
                BridgeType,
                rosrust::api::Topic,
            ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
            + Send
            + Sync
            + 'static,
    {
        match Self::parse_format(&data, &callback).await {
            Ok(_) => {}
            Err(e) => {
                error!(
                    "ROS1 Discovery: entry {}: processing error: {}",
                    data.as_str(),
                    e
                );
                debug_assert!(false);
            }
        }
    }

    async fn parse_format<F>(data: &KeyExpr<'_>, callback: &Arc<F>) -> Result<(), String>
    where
        F: Fn(
            BridgeType,
            rosrust::api::Topic,
        ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>,
    {
        let discovery = discovery_format::parse(data).map_err(|err| err.to_string())?;
        Self::handle_format(discovery, callback).await
    }

    async fn handle_format<F>(
        discovery: discovery_format::Parsed<'_>,
        callback: &Arc<F>,
    ) -> Result<(), String>
    where
        F: Fn(
            BridgeType,
            rosrust::api::Topic,
        ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>,
    {
        //let discovery_namespace = discovery.discovery_namespace().ok_or("No discovery_namespace present!")?;
        let datatype = discovery.data_type().ok_or("No data_type present!")?;
        let resource_class = discovery
            .resource_class()
            .ok_or("No resource_class present!")?
            .to_string();
        //let bridge_namespace = discovery.bridge_namespace().ok_or("No bridge_namespace present!")?.to_string();
        let topic = discovery.topic().ok_or("No topic present!")?;

        let ros1_topic = make_topic(datatype, topic)?;

        let b_type = match resource_class.as_str() {
            ROS1_DISCOVERY_INFO_PUBLISHERS_CLASS => BridgeType::Publisher,
            ROS1_DISCOVERY_INFO_SUBSCRIBERS_CLASS => BridgeType::Subscriber,
            ROS1_DISCOVERY_INFO_SERVICES_CLASS => BridgeType::Service,
            ROS1_DISCOVERY_INFO_CLIENTS_CLASS => BridgeType::Client,
            _ => {
                return Err("unexpected resource class!".to_string());
            }
        };

        callback(b_type, ros1_topic).await;
        Ok(())
    }
}

pub struct LocalResource {
    _declaration: AlohaDeclaration,
}
impl LocalResource {
    async fn new(
        discovery_namespace: &str,
        bridge_namespace: &str,
        resource_class: &str,
        topic: &rosrust::api::Topic,
        session: Arc<zenoh::Session>,
    ) -> LocalResource {
        // make proper discovery keyexpr
        let mut formatter = discovery_format::formatter();
        let discovery_keyexpr = zenoh::keformat!(
            formatter,
            discovery_namespace = discovery_namespace,
            resource_class = resource_class,
            data_type = topic.datatype.clone(),
            bridge_namespace = bridge_namespace,
            topic = make_zenoh_key(topic)
        )
        .unwrap();

        let _declaration =
            AlohaDeclaration::new(session, discovery_keyexpr, Duration::from_secs(1));

        Self { _declaration }
    }
}

pub struct LocalResources {
    session: Arc<zenoh::Session>,
    discovery_namespace: String,
    bridge_namespace: String,
}
impl LocalResources {
    pub fn new(
        discovery_namespace: String,
        bridge_namespace: String,
        session: Arc<zenoh::Session>,
    ) -> LocalResources {
        Self {
            session,
            discovery_namespace,
            bridge_namespace,
        }
    }

    pub async fn declare_with_type(
        &self,
        topic: &rosrust::api::Topic,
        b_type: BridgeType,
    ) -> LocalResource {
        match b_type {
            BridgeType::Publisher => self.declare_publisher(topic).await,
            BridgeType::Subscriber => self.declare_subscriber(topic).await,
            BridgeType::Service => self.declare_service(topic).await,
            BridgeType::Client => self.declare_client(topic).await,
        }
    }

    pub async fn declare_publisher(&self, topic: &rosrust::api::Topic) -> LocalResource {
        self.declare(topic, ROS1_DISCOVERY_INFO_PUBLISHERS_CLASS)
            .await
    }

    pub async fn declare_subscriber(&self, topic: &rosrust::api::Topic) -> LocalResource {
        self.declare(topic, ROS1_DISCOVERY_INFO_SUBSCRIBERS_CLASS)
            .await
    }

    pub async fn declare_service(&self, topic: &rosrust::api::Topic) -> LocalResource {
        self.declare(topic, ROS1_DISCOVERY_INFO_SERVICES_CLASS)
            .await
    }

    pub async fn declare_client(&self, topic: &rosrust::api::Topic) -> LocalResource {
        self.declare(topic, ROS1_DISCOVERY_INFO_CLIENTS_CLASS).await
    }

    //PRIVATE:
    pub async fn declare(
        &self,
        topic: &rosrust::api::Topic,
        resource_class: &str,
    ) -> LocalResource {
        LocalResource::new(
            &self.discovery_namespace,
            &self.bridge_namespace,
            resource_class,
            topic,
            self.session.clone(),
        )
        .await
    }
}

pub struct Discovery {
    _remote_resources: RemoteResources,
    local_resources: LocalResources,
}
impl Discovery {
    pub async fn new<F>(
        discovery_namespace: String,
        bridge_namespace: String,
        session: Arc<zenoh::Session>,
        on_discovered: F,
        on_lost: F,
    ) -> Self
    where
        F: Fn(
                BridgeType,
                rosrust::api::Topic,
            ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
            + Send
            + Sync
            + 'static,
    {
        Self {
            _remote_resources: RemoteResources::new(
                session.clone(),
                discovery_namespace.clone(),
                on_discovered,
                on_lost,
            )
            .await,
            local_resources: LocalResources::new(discovery_namespace, bridge_namespace, session),
        }
    }

    pub fn local_resources(&self) -> &LocalResources {
        &self.local_resources
    }
}

pub type TCallback = dyn Fn(BridgeType, rosrust::api::Topic) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
    + Send
    + Sync
    + 'static;

pub struct DiscoveryBuilder {
    discovery_namespace: String,
    bridge_namespace: String,
    session: Arc<zenoh::Session>,

    on_discovered: Option<Box<TCallback>>,
    on_lost: Option<Box<TCallback>>,
}

impl DiscoveryBuilder {
    pub fn new(
        discovery_namespace: String,
        bridge_namespace: String,
        session: Arc<zenoh::Session>,
    ) -> Self {
        Self {
            discovery_namespace,
            bridge_namespace,
            session,
            on_discovered: None,
            on_lost: None,
        }
    }

    pub fn on_discovered<F>(&mut self, on_discovered: F) -> &mut Self
    where
        F: Fn(
                BridgeType,
                rosrust::api::Topic,
            ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
            + Send
            + Sync
            + 'static,
    {
        self.on_discovered = Some(Box::new(on_discovered));
        self
    }
    pub fn on_lost<F>(&mut self, on_lost: F) -> &mut Self
    where
        F: Fn(
                BridgeType,
                rosrust::api::Topic,
            ) -> Box<dyn Future<Output = ()> + Unpin + Send + Sync>
            + Send
            + Sync
            + 'static,
    {
        self.on_lost = Some(Box::new(on_lost));
        self
    }

    pub async fn build(self) -> Discovery {
        Discovery::new(
            self.discovery_namespace,
            self.bridge_namespace,
            self.session,
            self.on_discovered
                .unwrap_or(Box::new(|_, _| Box::new(Box::pin(async {})))),
            self.on_lost
                .unwrap_or(Box::new(|_, _| Box::new(Box::pin(async {})))),
        )
        .await
    }
}
