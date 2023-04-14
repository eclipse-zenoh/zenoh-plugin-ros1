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

use std::{collections::{HashMap, hash_map::Entry, HashSet}, sync::Arc};

use super::{topic_bridge::{TopicBridge, self}, bridge_type::BridgeType, ros1_client, discovery::LocalResources, zenoh_client, topic_mapping::Ros1TopicMapping, ros1_to_zenoh_bridge_impl::BridgeStatus};


struct Bridges {
    publisher_bridges: HashMap<rosrust::api::Topic, TopicBridge>,
    subscriber_bridges: HashMap<rosrust::api::Topic, TopicBridge>,
    service_bridges: HashMap<rosrust::api::Topic, TopicBridge>,
    client_bridges: HashMap<rosrust::api::Topic, TopicBridge>
}
impl Bridges {
    fn new() -> Self {
        Self { 
            publisher_bridges: HashMap::new(),
            subscriber_bridges: HashMap::new(),
            service_bridges: HashMap::new(),
            client_bridges: HashMap::new()
        }
    }

    fn container_mut(&mut self, b_type: BridgeType) -> &mut HashMap<rosrust::api::Topic, TopicBridge> {
        match b_type {
            BridgeType::Publisher => &mut self.publisher_bridges,
            BridgeType::Subscriber => &mut self.subscriber_bridges,
            BridgeType::Service => &mut self.service_bridges,
            BridgeType::Client => &mut self.client_bridges,
        }
    }

    fn status(&self) -> BridgeStatus {
        let fill = |status: &mut (usize, usize), bridges: &HashMap<rosrust::api::Topic, TopicBridge>| {
            for (_topic, bridge) in bridges.iter() {
                status.0 += 1;
                if bridge.is_bridging() {
                    status.1 += 1;
                }
            }
        };

        let mut result = BridgeStatus::default();
        fill(&mut result.ros_clients, &self.client_bridges);
        fill(&mut result.ros_publishers, &self.publisher_bridges);
        fill(&mut result.ros_services, &self.service_bridges);
        fill(&mut result.ros_subscribers, &self.subscriber_bridges);
        return result;
    }

    fn clear(&mut self) {
        self.publisher_bridges.clear();
        self.subscriber_bridges.clear();
        self.service_bridges.clear();
        self.client_bridges.clear();
    }
}


struct Access<'a> {
    container: &'a mut HashMap<rosrust::api::Topic, TopicBridge>,
    b_type: BridgeType,
    ros1_client: Arc<ros1_client::Ros1Client>, 
    zenoh_client: Arc<zenoh_client::ZenohClient>,
    declaration_interface: Arc<LocalResources>
}

impl<'a> Access<'a> {
    fn new(b_type: BridgeType,
           container: &'a mut HashMap<rosrust::api::Topic, TopicBridge>, 
           ros1_client: Arc<ros1_client::Ros1Client>,
           zenoh_client: Arc<zenoh_client::ZenohClient>,
           declaration_interface: Arc<LocalResources>) -> Self {
        Self { 
            container,
            b_type,
            ros1_client,
            zenoh_client,
            declaration_interface }
    }
}


pub struct ComplementaryElementAccessor<'a> {
    access: Access<'a>
}

impl<'a> ComplementaryElementAccessor<'a> {
    fn new(b_type: BridgeType,
           container: &'a mut HashMap<rosrust::api::Topic, TopicBridge>, 
           ros1_client: Arc<ros1_client::Ros1Client>,
           zenoh_client: Arc<zenoh_client::ZenohClient>,
           declaration_interface: Arc<LocalResources>) -> Self {
        Self { access: Access::new(b_type, container, ros1_client, zenoh_client, declaration_interface) }
    }

    pub async fn complementary_entity_lost(&mut self, topic: rosrust::api::Topic) {
        match self.access.container.entry(topic) {
            Entry::Occupied(mut val) => {
                let bridge = val.get_mut();
                if bridge.set_has_complementary_in_zenoh(false).await && !bridge.is_actual() {
                    val.remove_entry();
                }
            },
            Entry::Vacant(_) => {},
        }
    }

    pub async fn complementary_entity_discovered(&mut self, topic: rosrust::api::Topic) {
        match self.access.container.entry(topic) {
            Entry::Occupied(mut val) => {
                val.get_mut().set_has_complementary_in_zenoh(true).await;
            }
            Entry::Vacant(val) => {
                let key = val.key().clone();
                val.insert(TopicBridge::new(
                    key,
                    self.access.b_type,
                    self.access.declaration_interface.clone(),
                    self.access.ros1_client.clone(),
                    self.access.zenoh_client.clone(),
                    topic_bridge::BridgingMode::Automatic
                )).set_has_complementary_in_zenoh(true).await;
            }
        }
    }
}


pub struct ElementAccessor<'a> {
    access: Access<'a>
}

impl<'a> ElementAccessor<'a> {
    fn new(b_type: BridgeType,
           container: &'a mut HashMap<rosrust::api::Topic, TopicBridge>, 
           ros1_client: Arc<ros1_client::Ros1Client>,
           zenoh_client: Arc<zenoh_client::ZenohClient>,
           declaration_interface: Arc<LocalResources>) -> Self {
        Self { access: Access::new(b_type, container, ros1_client, zenoh_client, declaration_interface) }
    }

    async fn receive_ros1_state(&mut self,
                                part_of_ros_state: &mut HashSet<rosrust::api::Topic>) -> bool {
        let mut smth_changed = false;
        // Run through bridges and actualize their state based on ROS1 state, removing corresponding entries from ROS1 state.
        // As a result of this cycle, part_of_ros_state will contain only new topics that doesn't have corresponding bridge. 
        for (topic, bridge) in self.access.container.iter_mut() {
            smth_changed |= bridge.set_present_in_ros1(part_of_ros_state.take(topic).is_some()).await;
        }

        // erase all non-actual bridges
        {
            let size_before_retain = self.access.container.len();
            self.access.container.retain(|_topic, bridge| {return bridge.is_actual();});
            smth_changed |= size_before_retain != self.access.container.len();
        }

        // run through the topics and create corresponding bridges
        for ros_topic in part_of_ros_state.iter() {
            match self.access.container.entry(ros_topic.clone()) {
                Entry::Occupied(_val) => {
                    debug_assert!(false); // that shouldn't happen
                }
                Entry::Vacant(val) => {
                    let inserted = val.insert(TopicBridge::new(
                        ros_topic.clone(),
                        self.access.b_type,
                        self.access.declaration_interface.clone(),
                        self.access.ros1_client.clone(),
                        self.access.zenoh_client.clone(),
                        topic_bridge::BridgingMode::Automatic));
                    inserted.set_present_in_ros1(true).await;
                    smth_changed = true;
                }
            } 
        }
        return smth_changed;
    }
}


pub struct TypeAccessor<'a> {
    storage: &'a mut BridgesStorage,
    _v: bool
}
impl<'a> TypeAccessor<'a> {
    fn new(storage: &'a mut BridgesStorage) -> Self { Self { storage, _v: false } }

    pub fn complementary_for(&'a mut self, b_type: BridgeType) -> ComplementaryElementAccessor<'a> {
        let b_type = match b_type {
            BridgeType::Publisher => BridgeType::Subscriber,
            BridgeType::Subscriber => BridgeType::Publisher,
            BridgeType::Service => BridgeType::Client,
            BridgeType::Client => BridgeType::Service
        };
        ComplementaryElementAccessor::new(
            b_type,
            self.storage.bridges.container_mut(b_type),
            self.storage.ros1_client.clone(),
            self.storage.zenoh_client.clone(),
            self.storage.declaration_interface.clone()
        )
    }

    pub fn for_type(&'a mut self, b_type: BridgeType) -> ElementAccessor<'a> {
        ElementAccessor::new(
            b_type,
            self.storage.bridges.container_mut(b_type),
            self.storage.ros1_client.clone(),
            self.storage.zenoh_client.clone(),
            self.storage.declaration_interface.clone()
        )
    }
}



pub struct BridgesStorage {
    bridges: Bridges,

    ros1_client: Arc<ros1_client::Ros1Client>, 
    zenoh_client: Arc<zenoh_client::ZenohClient>,
    declaration_interface: Arc<LocalResources>
}
impl BridgesStorage {
    pub fn new(ros1_client: Arc<ros1_client::Ros1Client>, 
           zenoh_client: Arc<zenoh_client::ZenohClient>,
           declaration_interface: Arc<LocalResources>) -> Self {
        Self { 
            bridges: Bridges::new(),

            ros1_client,
            zenoh_client,
            declaration_interface
        }
    }

    pub fn bridges(&mut self) -> TypeAccessor {
        TypeAccessor::new(self)
    }

    pub async fn receive_ros1_state (&mut self,
                                 ros1_state: &mut Ros1TopicMapping) -> bool {
        
        let mut smth_changed = self.bridges().for_type(BridgeType::Publisher).receive_ros1_state(&mut ros1_state.published).await;
        smth_changed |= self.bridges().for_type(BridgeType::Service).receive_ros1_state(&mut ros1_state.serviced).await;
        smth_changed |= self.bridges().for_type(BridgeType::Subscriber).receive_ros1_state(&mut ros1_state.subscribed).await;
        return smth_changed;
    }

    pub fn status(&self) -> BridgeStatus {
        return self.bridges.status();
    }

    pub fn clear(&mut self) {
        return self.bridges.clear();
    }
}


