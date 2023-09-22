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

use log::debug;
use rosrust::{self, RawMessageDescription};
use zenoh_core::{zerror, zresult::ZResult};

use super::topic_descriptor::TopicDescriptor;

pub struct Ros1Client {
    pub ros: rosrust::api::Ros,
}

impl Ros1Client {
    // PUBLIC
    pub fn new(name: &str, master_uri: &str) -> ZResult<Ros1Client> {
        Ok(Ros1Client {
            ros: rosrust::api::Ros::new_raw(
                master_uri,
                &rosrust::api::resolve::hostname(),
                &rosrust::api::resolve::namespace(),
                name,
            )
            .map_err(|e| zerror!("{e}"))?,
        })
    }

    pub fn subscribe<T, F>(
        &self,
        topic: &TopicDescriptor,
        callback: F,
    ) -> rosrust::api::error::Result<rosrust::Subscriber>
    where
        T: rosrust::Message,
        F: Fn(T) + Send + 'static,
    {
        let description = RawMessageDescription {
            msg_definition: "*".to_string(),
            md5sum: topic.md5.clone(),
            msg_type: topic.datatype.clone(),
        };
        self.ros.subscribe_with_ids_and_headers(
            &topic.name,
            0,
            move |data, _| callback(data),
            |_| (),
            Some(description),
        )
    }

    pub fn publish(
        &self,
        topic: &TopicDescriptor,
    ) -> rosrust::api::error::Result<rosrust::Publisher<rosrust::RawMessage>> {
        let description = RawMessageDescription {
            msg_definition: "*".to_string(),
            md5sum: topic.md5.clone(),
            msg_type: topic.datatype.clone(),
        };
        self.ros
            .publish_with_description(&topic.name, 0, description)
    }

    pub fn client(
        &self,
        topic: &TopicDescriptor,
    ) -> rosrust::api::error::Result<rosrust::Client<rosrust::RawMessage>> {
        self.ros.client::<rosrust::RawMessage>(&topic.name)
    }

    pub fn service<T, F>(
        &self,
        topic: &TopicDescriptor,
        handler: F,
    ) -> rosrust::api::error::Result<rosrust::Service>
    where
        T: rosrust::ServicePair,
        F: Fn(T::Request) -> rosrust::ServiceResult<T::Response> + Send + Sync + 'static,
    {
        let description = RawMessageDescription {
            msg_definition: "*".to_string(),
            md5sum: topic.md5.clone(),
            msg_type: topic.datatype.clone(),
        };
        self.ros
            .service_with_description::<T, F>(&topic.name, handler, description)
    }

    pub fn state(&self) -> rosrust::api::error::Response<rosrust::api::SystemState> {
        self.filter(self.ros.state())
    }

    pub fn topic_types(&self) -> rosrust::api::error::Response<Vec<rosrust::api::Topic>> {
        self.ros.topics()
    }

    // PRIVATE
    /**
     * Filter out topics, which are published\subscribed\serviced only by the bridge itself
     */
    fn filter(
        &self,
        mut state: rosrust::api::error::Response<rosrust::api::SystemState>,
    ) -> rosrust::api::error::Response<rosrust::api::SystemState> {
        debug!("system state before filter: {:#?}", state);
        if let Ok(value) = state.as_mut() {
            let name = self.ros.name();

            let retain_lambda = |x: &rosrust::api::TopicData| {
                x.connections.len() > 1 || (x.connections.len() == 1 && x.connections[0] != name)
            };

            value.publishers.retain(retain_lambda);
            value.subscribers.retain(retain_lambda);
            value.services.retain(retain_lambda);
        }
        debug!("system state after filter: {:#?}", state);
        state
    }
}
