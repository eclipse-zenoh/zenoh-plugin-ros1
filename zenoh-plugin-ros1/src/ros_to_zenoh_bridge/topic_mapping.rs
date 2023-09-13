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
use log::debug;

use super::service_cache::Ros1ServiceCache;

#[derive(Debug)]
pub struct Ros1TopicMapping {
    pub published: HashSet<rosrust::api::Topic>,
    pub subscribed: HashSet<rosrust::api::Topic>,
    pub serviced: HashSet<rosrust::api::Topic>,
}
impl Ros1TopicMapping {
    pub fn topic_mapping(
        ros1_client: &ros1_client::Ros1Client,
        ros1_service_cache: &mut Ros1ServiceCache,
    ) -> rosrust::api::error::Response<Ros1TopicMapping> {
        match ros1_client.state() {
            Ok(state_val) => match ros1_client.topic_types() {
                Ok(topics_val) => {
                    debug!("topics: {:#?}", topics_val);
                    Ok(Ros1TopicMapping::new(
                        state_val,
                        &topics_val,
                        ros1_service_cache,
                    ))
                }
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    // PRIVATE:
    fn new(
        state: rosrust::api::SystemState,
        topics: &[rosrust::api::Topic],
        ros1_service_cache: &mut Ros1ServiceCache,
    ) -> Ros1TopicMapping {
        let mut result = Ros1TopicMapping {
            published: HashSet::new(),
            subscribed: HashSet::new(),
            serviced: HashSet::new(),
        };

        Ros1TopicMapping::fill(&mut result.subscribed, &state.subscribers, topics);
        Ros1TopicMapping::fill(&mut result.published, &state.publishers, topics);
        Ros1TopicMapping::fill_services(&mut result.serviced, state.services, ros1_service_cache);

        result
    }

    fn fill(
        dst: &mut HashSet<rosrust::api::Topic>,
        data: &[rosrust::api::TopicData],
        topics: &[rosrust::api::Topic],
    ) {
        for item in data.iter() {
            let topic = topics.iter().find(|x| x.name == item.name);
            match topic {
                None => {
                    debug!("Unable to find datatype for topic {}", item.name);
                    dst.insert(rosrust::api::Topic {
                        name: item.name.clone(),
                        datatype: "*".to_string(),
                    });
                }
                Some(val) => {
                    dst.insert(val.clone());
                }
            }
        }
    }

    fn fill_services(
        dst: &mut HashSet<rosrust::api::Topic>,
        data: Vec<rosrust::api::TopicData>,
        ros1_service_cache: &mut Ros1ServiceCache,
    ) {
        for service in data {
            for node in service.connections {
                match ros1_service_cache.resolve_datatype(service.name.clone(), node) {
                    Ok(datatype) => {
                        dst.insert(rosrust::api::Topic {
                            name: service.name.clone(),
                            datatype,
                        });
                    }
                    Err(e) => {
                        debug!(
                            "Error finding datatype for service topic {}: {e}",
                            service.name.clone()
                        );
                    }
                }
            }
        }
    }
}
