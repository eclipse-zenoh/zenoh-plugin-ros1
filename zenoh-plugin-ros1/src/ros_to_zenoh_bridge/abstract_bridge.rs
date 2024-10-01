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

use rosrust::RawMessageDescription;
use tracing::{debug, error, info};
use zenoh::{internal::bail, key_expr::keyexpr, Result as ZResult, Wait};

use super::{
    bridge_type::BridgeType, ros1_client, topic_descriptor::TopicDescriptor,
    topic_utilities::make_zenoh_key, zenoh_client,
};
use crate::{blockon_runtime, spawn_blocking_runtime, spawn_runtime};

pub struct AbstractBridge {
    _impl: BridgeIml,
}

impl AbstractBridge {
    pub async fn new(
        b_type: BridgeType,
        topic: &TopicDescriptor,
        ros1_client: &ros1_client::Ros1Client,
        zenoh_client: &Arc<zenoh_client::ZenohClient>,
    ) -> ZResult<Self> {
        let _impl = {
            match b_type {
                BridgeType::Publisher => {
                    BridgeIml::Pub(Ros1ToZenoh::new(topic, ros1_client, zenoh_client).await?)
                }
                BridgeType::Subscriber => {
                    BridgeIml::Sub(ZenohToRos1::new(topic, ros1_client, zenoh_client).await?)
                }
                BridgeType::Service => BridgeIml::Service(
                    Ros1ToZenohService::new(topic, ros1_client, zenoh_client).await?,
                ),
                BridgeType::Client => BridgeIml::Client(
                    Ros1ToZenohClient::new(topic, ros1_client, zenoh_client.clone()).await?,
                ),
            }
        };
        Ok(Self { _impl })
    }
}

enum BridgeIml {
    Client(Ros1ToZenohClient),
    Service(Ros1ToZenohService),
    Pub(Ros1ToZenoh),
    Sub(ZenohToRos1),
}

struct Ros1ToZenohClient {
    _service: rosrust::Service,
}
impl Ros1ToZenohClient {
    async fn new(
        topic: &TopicDescriptor,
        ros1_client: &ros1_client::Ros1Client,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
    ) -> ZResult<Ros1ToZenohClient> {
        info!("Creating ROS1 -> Zenoh Client bridge for {:?}", topic);

        let zenoh_key = make_zenoh_key(topic);
        match ros1_client.service::<rosrust::RawMessage, _>(
            topic,
            move |q| -> rosrust::ServiceResult<rosrust::RawMessage> {
                return Self::on_query(&zenoh_key, q, zenoh_client.as_ref());
            },
        ) {
            Ok(service) => Ok(Ros1ToZenohClient { _service: service }),
            Err(e) => {
                bail!("Ros error: {}", e)
            }
        }
    }

    //PRIVATE:
    fn on_query(
        key: &keyexpr,
        query: rosrust::RawMessage,
        zenoh_client: &zenoh_client::ZenohClient,
    ) -> rosrust::ServiceResult<rosrust::RawMessage> {
        return blockon_runtime(Self::do_zenoh_query(key, query, zenoh_client));
    }

    async fn do_zenoh_query(
        key: &keyexpr,
        query: rosrust::RawMessage,
        zenoh_client: &zenoh_client::ZenohClient,
    ) -> rosrust::ServiceResult<rosrust::RawMessage> {
        match zenoh_client.make_query_sync(key, query.0).await {
            Ok(reply) => match reply.recv_async().await {
                Ok(r) => match r.result() {
                    Ok(sample) => {
                        let data = sample.payload().to_bytes().into_owned();
                        debug!("Zenoh -> ROS1: sending {} bytes!", data.len());
                        Ok(rosrust::RawMessage(data))
                    }
                    Err(e) => {
                        let error = format!("{:?}", e);
                        error!(
                            "ROS1 -> Zenoh Client: received Zenoh Query with error: {:?}",
                            error
                        );
                        Err(error)
                    }
                },
                Err(e) => {
                    error!(
                        "ROS1 -> Zenoh Client: error while receiving reply to Zenoh Query: {}",
                        e
                    );
                    let error = e.to_string();
                    Err(error)
                }
            },
            Err(e) => {
                error!(
                    "ROS1 -> Zenoh Client: error while creating Zenoh Query: {}",
                    e
                );
                let error = e.to_string();
                Err(error)
            }
        }
    }
}

struct Ros1ToZenohService {
    _queryable: zenoh::query::Queryable<()>,
}
impl Ros1ToZenohService {
    async fn new<'b>(
        topic: &TopicDescriptor,
        ros1_client: &ros1_client::Ros1Client,
        zenoh_client: &'b zenoh_client::ZenohClient,
    ) -> ZResult<Ros1ToZenohService> {
        info!(
            "Creating ROS1 -> Zenoh Service bridge for topic {}, datatype {}",
            topic.name, topic.datatype
        );

        match ros1_client.client(topic) {
            Ok(client) => {
                let client_in_arc = Arc::new(client);
                let topic_in_arc = Arc::new(topic.clone());
                let queryable = zenoh_client
                    .make_queryable(make_zenoh_key(topic), move |query| {
                        spawn_runtime(Self::on_query(
                            client_in_arc.clone(),
                            query,
                            topic_in_arc.clone(),
                        ));
                    })
                    .await?;
                Ok(Ros1ToZenohService {
                    _queryable: queryable,
                })
            }
            Err(e) => {
                bail!("Ros error: {}", e.to_string())
            }
        }
    }

    //PRIVATE:
    async fn on_query(
        ros1_client: Arc<rosrust::Client<rosrust::RawMessage>>,
        query: zenoh::query::Query,
        topic: Arc<TopicDescriptor>,
    ) {
        match query.payload() {
            Some(val) => {
                let payload = val.to_bytes().into_owned();
                debug!(
                    "ROS1 -> Zenoh Service: got query of {} bytes!",
                    payload.len()
                );
                Self::process_query(ros1_client, query, payload, topic).await;
            }
            None => {
                error!("ROS1 -> Zenoh Service: got query without value!");
            }
        }
    }

    async fn process_query(
        ros1_client: Arc<rosrust::Client<rosrust::RawMessage>>,
        query: zenoh::query::Query,
        payload: Vec<u8>,
        topic: Arc<TopicDescriptor>,
    ) {
        // rosrust is synchronous, so we will use spawn_blocking. If there will be an async mode some day for the rosrust,
        // than reply_to_query can be refactored to async very easily
        let res = spawn_blocking_runtime(move || {
            let description = RawMessageDescription {
                msg_definition: String::from("*"),
                md5sum: topic.md5.clone(),
                msg_type: topic.datatype.clone(),
            };
            ros1_client.req_with_description(&rosrust::RawMessage(payload), description)
        })
        .await
        .expect("Unable to compete the task");
        match Self::reply_to_query(res, &query).await {
            Ok(_) => {}
            Err(e) => {
                error!(
                    "ROS1 -> Zenoh Service: error replying to query on {}: {}",
                    query.key_expr(),
                    e
                );
            }
        }
    }

    async fn reply_to_query(
        res: rosrust::error::tcpros::Result<rosrust::ServiceResult<rosrust::RawMessage>>,
        query: &zenoh::query::Query,
    ) -> ZResult<()> {
        match res {
            Ok(reply) => match reply {
                Ok(reply_message) => {
                    debug!(
                        "ROS1 -> Zenoh Service: got reply of {} bytes!",
                        reply_message.0.len()
                    );
                    query
                        .reply(query.key_expr().clone(), reply_message.0)
                        .await?;
                }
                Err(e) => {
                    error!(
                        "ROS1 -> Zenoh Service: got reply from ROS1 Service with error: {}",
                        e
                    );
                    query.reply(query.key_expr().clone(), e).await?;
                }
            },
            Err(e) => {
                error!(
                    "ROS1 -> Zenoh Service: error while sending request to ROS1 Service: {}",
                    e
                );
                let error = e.to_string();
                query.reply(query.key_expr().clone(), error).await?;
            }
        }
        Ok(())
    }
}

struct Ros1ToZenoh {
    _subscriber: rosrust::Subscriber,
}
impl Ros1ToZenoh {
    async fn new<'b>(
        topic: &TopicDescriptor,
        ros1_client: &ros1_client::Ros1Client,
        zenoh_client: &'b zenoh_client::ZenohClient,
    ) -> ZResult<Ros1ToZenoh> {
        info!(
            "Creating ROS1 -> Zenoh bridge for topic {}, datatype {}",
            topic.name, topic.datatype
        );

        let publisher = zenoh_client.publish(make_zenoh_key(topic)).await?;
        match ros1_client.subscribe(topic, move |msg: rosrust::RawMessage| {
            debug!("ROS1 -> Zenoh: sending {} bytes!", msg.0.len());
            match publisher.put(msg.0).wait() {
                Ok(_) => {}
                Err(e) => {
                    error!("ROS1 -> Zenoh: error publishing: {}", e);
                }
            }
        }) {
            Ok(subscriber) => Ok(Ros1ToZenoh {
                _subscriber: subscriber,
            }),
            Err(e) => {
                bail!("Ros error: {}", e.to_string())
            }
        }
    }
}

struct ZenohToRos1 {
    _subscriber: zenoh::pubsub::Subscriber<()>,
}
impl ZenohToRos1 {
    async fn new(
        topic: &TopicDescriptor,
        ros1_client: &ros1_client::Ros1Client,
        zenoh_client: &Arc<zenoh_client::ZenohClient>,
    ) -> ZResult<Self> {
        info!(
            "Creating Zenoh -> ROS1 bridge for topic {}, datatype {}",
            topic.name, topic.datatype
        );

        match ros1_client.publish(topic) {
            Ok(publisher) => {
                let publisher_in_arc = Arc::new(publisher);
                let subscriber = zenoh_client
                    .subscribe(make_zenoh_key(topic), move |sample| {
                        let publisher_in_arc_cloned = publisher_in_arc.clone();
                        spawn_blocking_runtime(move || {
                            let data = sample.payload().to_bytes();
                            debug!("Zenoh -> ROS1: sending {} bytes!", data.len());
                            match publisher_in_arc_cloned
                                .send(rosrust::RawMessage(data.into_owned()))
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    error!("Zenoh -> ROS1: error publishing: {}", e);
                                }
                            }
                        });
                    })
                    .await?;
                Ok(ZenohToRos1 {
                    _subscriber: subscriber,
                })
            }
            Err(e) => {
                bail!("Ros error: {}", e.to_string())
            }
        }
    }
}
