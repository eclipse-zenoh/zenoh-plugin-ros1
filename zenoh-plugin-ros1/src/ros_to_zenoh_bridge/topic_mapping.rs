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

use std::collections::HashSet;

use crate::ros_to_zenoh_bridge::ros1_client;

use super::{resource_cache::Ros1ResourceCache, topic_descriptor::TopicDescriptor};

#[derive(Debug)]
pub struct Ros1TopicMapping {
    pub published: HashSet<TopicDescriptor>,
    pub subscribed: HashSet<TopicDescriptor>,
    pub serviced: HashSet<TopicDescriptor>,
}
impl Ros1TopicMapping {
    pub fn topic_mapping(
        ros1_client: &ros1_client::Ros1Client,
        ros1_service_cache: &mut Ros1ResourceCache,
    ) -> rosrust::api::error::Response<Ros1TopicMapping> {
        match ros1_client.state() {
            Ok(state_val) => Ok(Ros1TopicMapping::new(state_val, ros1_service_cache)),
            Err(e) => Err(e),
        }
    }

    // PRIVATE:
    fn new(
        state: rosrust::api::SystemState,
        ros1_service_cache: &mut Ros1ResourceCache,
    ) -> Ros1TopicMapping {
        let mut result = Ros1TopicMapping {
            published: HashSet::new(),
            subscribed: HashSet::default(),
            serviced: HashSet::new(),
        };

        // run through subscriber topics, resolve all available data formats
        // and fill 'result.subscribed' with unique combinations of topic_name + format descriptions
        for subscriber_topic in state.subscribers {
            for node in subscriber_topic.connections {
                if let Ok((datatype, md5)) = ros1_service_cache
                    .resolve_subscriber_parameters(subscriber_topic.name.clone(), node)
                {
                    result.subscribed.insert(TopicDescriptor {
                        name: subscriber_topic.name.clone(),
                        datatype,
                        md5,
                    });
                }
            }
        }

        // run through publisher topics, resolve all available data formats
        // and fill 'result.published' with unique combinations of topic_name + format descriptions
        for publisher_topic in state.publishers {
            for node in publisher_topic.connections {
                if let Ok((datatype, md5)) = ros1_service_cache
                    .resolve_publisher_parameters(publisher_topic.name.clone(), node)
                {
                    result.published.insert(TopicDescriptor {
                        name: publisher_topic.name.clone(),
                        datatype,
                        md5,
                    });
                }
            }
        }

        // run through service topics, resolve all available data formats
        // and fill 'result.serviced' with unique combinations of topic_name + format descriptions
        for service_topic in state.services {
            for node in service_topic.connections {
                if let Ok((datatype, md5)) =
                    ros1_service_cache.resolve_service_parameters(service_topic.name.clone(), node)
                {
                    result.serviced.insert(TopicDescriptor {
                        name: service_topic.name.clone(),
                        datatype,
                        md5,
                    });
                }
            }
        }
        result
    }
}
