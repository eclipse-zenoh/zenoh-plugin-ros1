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
use rosrust;

pub struct Ros1Client {
    ros: rosrust::api::Ros,
}

impl Ros1Client {
    // PUBLIC
    pub fn new(name: &str) -> Ros1Client {
        Ros1Client {
            ros: rosrust::api::Ros::new(name).unwrap(),
        }
    }

    pub fn subscribe<T, F>(
        &self,
        topic: &rosrust::api::Topic,
        callback: F,
    ) -> rosrust::api::error::Result<rosrust::Subscriber>
    where
        T: rosrust::Message,
        F: Fn(T) + Send + 'static,
    {
        self.ros.subscribe(&topic.name, 0, callback)
    }

    pub fn publish(
        &self,
        topic: &rosrust::api::Topic,
    ) -> rosrust::api::error::Result<rosrust::Publisher<rosrust::RawMessage>> {
        self.ros.publish(&topic.name, 0)
    }

    pub fn client(
        &self,
        topic: &rosrust::api::Topic,
    ) -> rosrust::api::error::Result<rosrust::Client<rosrust::RawMessage>> {
        self.ros.client::<rosrust::RawMessage>(&topic.name)
    }

    pub fn service<T, F>(
        &self,
        topic: &rosrust::api::Topic,
        handler: F,
    ) -> rosrust::api::error::Result<rosrust::Service>
    where
        T: rosrust::ServicePair,
        F: Fn(T::Request) -> rosrust::ServiceResult<T::Response> + Send + Sync + 'static,
    {
        self.ros.service::<T, F>(&topic.name, handler)
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
        match state.as_mut() {
            Ok(value) => {
                let name = self.ros.name();

                let retain_lambda = |x: &rosrust::api::TopicData| {
                    return x.connections.len() > 1
                        || (x.connections.len() == 1 && x.connections[0] != name);
                };

                value.publishers.retain(retain_lambda);
                value.subscribers.retain(retain_lambda);
                value.services.retain(retain_lambda);
            }
            Err(_) => {}
        }
        debug!("system state after filter: {:#?}", state);
        return state;
    }
}
