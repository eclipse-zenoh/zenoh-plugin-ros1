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

use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::Arc,
};

use super::{
    bridge_type::BridgeType,
    bridging_mode::{bridging_mode, BridgingMode},
    discovery::LocalResources,
    ros1_client,
    ros1_to_zenoh_bridge_impl::BridgeStatus,
    topic_bridge::TopicBridge,
    topic_descriptor::TopicDescriptor,
    topic_mapping::Ros1TopicMapping,
    zenoh_client,
};

struct Bridges {
    publisher_bridges: HashMap<TopicDescriptor, TopicBridge>,
    subscriber_bridges: HashMap<TopicDescriptor, TopicBridge>,
    service_bridges: HashMap<TopicDescriptor, TopicBridge>,
    client_bridges: HashMap<TopicDescriptor, TopicBridge>,
}
impl Bridges {
    fn new() -> Self {
        Self {
            publisher_bridges: HashMap::new(),
            subscriber_bridges: HashMap::new(),
            service_bridges: HashMap::new(),
            client_bridges: HashMap::new(),
        }
    }

    fn container_mut(&mut self, b_type: BridgeType) -> &mut HashMap<TopicDescriptor, TopicBridge> {
        match b_type {
            BridgeType::Publisher => &mut self.publisher_bridges,
            BridgeType::Subscriber => &mut self.subscriber_bridges,
            BridgeType::Service => &mut self.service_bridges,
            BridgeType::Client => &mut self.client_bridges,
        }
    }

    fn status(&self) -> BridgeStatus {
        let fill = |status: &mut (usize, usize),
                    bridges: &HashMap<TopicDescriptor, TopicBridge>| {
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
        result
    }

    fn clear(&mut self) {
        self.publisher_bridges.clear();
        self.subscriber_bridges.clear();
        self.service_bridges.clear();
        self.client_bridges.clear();
    }
}

struct Access<'a> {
    container: &'a mut HashMap<TopicDescriptor, TopicBridge>,
    b_type: BridgeType,
    ros1_client: Arc<ros1_client::Ros1Client>,
    zenoh_client: Arc<zenoh_client::ZenohClient>,
    declaration_interface: Arc<LocalResources>,
}

impl<'a> Access<'a> {
    fn new(
        b_type: BridgeType,
        container: &'a mut HashMap<TopicDescriptor, TopicBridge>,
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        declaration_interface: Arc<LocalResources>,
    ) -> Self {
        Self {
            container,
            b_type,
            ros1_client,
            zenoh_client,
            declaration_interface,
        }
    }
}

pub struct ComplementaryElementAccessor<'a> {
    access: Access<'a>,
}

impl<'a> ComplementaryElementAccessor<'a> {
    fn new(
        b_type: BridgeType,
        container: &'a mut HashMap<TopicDescriptor, TopicBridge>,
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        declaration_interface: Arc<LocalResources>,
    ) -> Self {
        Self {
            access: Access::new(
                b_type,
                container,
                ros1_client,
                zenoh_client,
                declaration_interface,
            ),
        }
    }

    pub async fn complementary_entity_lost(&mut self, topic: TopicDescriptor) {
        match self.access.container.entry(topic) {
            Entry::Occupied(mut val) => {
                let bridge = val.get_mut();
                if bridge.set_has_complementary_in_zenoh(false).await && !bridge.is_actual() {
                    val.remove_entry();
                }
            }
            Entry::Vacant(_) => {}
        }
    }

    pub async fn complementary_entity_discovered(&mut self, topic: TopicDescriptor) {
        let b_mode = bridging_mode(self.access.b_type, topic.name.as_str());
        if b_mode != BridgingMode::Disabled {
            match self.access.container.entry(topic) {
                Entry::Occupied(mut val) => {
                    val.get_mut().set_has_complementary_in_zenoh(true).await;
                }
                Entry::Vacant(val) => {
                    let key = val.key().clone();
                    let inserted = val.insert(TopicBridge::new(
                        key,
                        self.access.b_type,
                        self.access.declaration_interface.clone(),
                        self.access.ros1_client.clone(),
                        self.access.zenoh_client.clone(),
                        b_mode,
                    ));
                    inserted.set_has_complementary_in_zenoh(true).await;
                }
            }
        }
    }
}

pub struct ElementAccessor<'a> {
    access: Access<'a>,
}

impl<'a> ElementAccessor<'a> {
    fn new(
        b_type: BridgeType,
        container: &'a mut HashMap<TopicDescriptor, TopicBridge>,
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        declaration_interface: Arc<LocalResources>,
    ) -> Self {
        Self {
            access: Access::new(
                b_type,
                container,
                ros1_client,
                zenoh_client,
                declaration_interface,
            ),
        }
    }

    async fn receive_ros1_state(
        &mut self,
        part_of_ros_state: &mut HashSet<TopicDescriptor>,
    ) -> bool {
        let mut smth_changed = false;
        // Run through bridges and actualize their state based on ROS1 state, removing corresponding entries from ROS1 state.
        // As a result of this cycle, part_of_ros_state will contain only new topics that doesn't have corresponding bridge.
        for (topic, bridge) in self.access.container.iter_mut() {
            smth_changed |= bridge
                .set_present_in_ros1(part_of_ros_state.take(topic).is_some())
                .await;
        }

        // erase all non-actual bridges
        {
            let size_before_retain = self.access.container.len();
            self.access
                .container
                .retain(|_topic, bridge| bridge.is_actual());
            smth_changed |= size_before_retain != self.access.container.len();
        }

        // run through the topics and create corresponding bridges
        for topic in part_of_ros_state.iter() {
            let b_mode = bridging_mode(self.access.b_type, &topic.name);
            if b_mode != BridgingMode::Disabled {
                match self.access.container.entry(topic.clone()) {
                    Entry::Occupied(_val) => {
                        debug_assert!(false); // that shouldn't happen
                    }
                    Entry::Vacant(val) => {
                        let inserted = val.insert(TopicBridge::new(
                            topic.clone(),
                            self.access.b_type,
                            self.access.declaration_interface.clone(),
                            self.access.ros1_client.clone(),
                            self.access.zenoh_client.clone(),
                            b_mode,
                        ));
                        inserted.set_present_in_ros1(true).await;
                        smth_changed = true;
                    }
                }
            }
        }
        smth_changed
    }
}

pub struct TypeAccessor<'a> {
    storage: &'a mut BridgesStorage,
    _v: bool,
}
impl<'a> TypeAccessor<'a> {
    fn new(storage: &'a mut BridgesStorage) -> Self {
        Self { storage, _v: false }
    }

    pub fn complementary_for(&'a mut self, b_type: BridgeType) -> ComplementaryElementAccessor<'a> {
        let b_type = match b_type {
            BridgeType::Publisher => BridgeType::Subscriber,
            BridgeType::Subscriber => BridgeType::Publisher,
            BridgeType::Service => BridgeType::Client,
            BridgeType::Client => BridgeType::Service,
        };
        ComplementaryElementAccessor::new(
            b_type,
            self.storage.bridges.container_mut(b_type),
            self.storage.ros1_client.clone(),
            self.storage.zenoh_client.clone(),
            self.storage.declaration_interface.clone(),
        )
    }

    pub fn for_type(&'a mut self, b_type: BridgeType) -> ElementAccessor<'a> {
        ElementAccessor::new(
            b_type,
            self.storage.bridges.container_mut(b_type),
            self.storage.ros1_client.clone(),
            self.storage.zenoh_client.clone(),
            self.storage.declaration_interface.clone(),
        )
    }
}

pub struct BridgesStorage {
    bridges: Bridges,

    ros1_client: Arc<ros1_client::Ros1Client>,
    zenoh_client: Arc<zenoh_client::ZenohClient>,
    declaration_interface: Arc<LocalResources>,
}
impl BridgesStorage {
    pub fn new(
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        declaration_interface: Arc<LocalResources>,
    ) -> Self {
        Self {
            bridges: Bridges::new(),

            ros1_client,
            zenoh_client,
            declaration_interface,
        }
    }

    pub fn bridges(&mut self) -> TypeAccessor {
        TypeAccessor::new(self)
    }

    pub async fn receive_ros1_state(&mut self, ros1_state: &mut Ros1TopicMapping) -> bool {
        let mut smth_changed = self
            .bridges()
            .for_type(BridgeType::Publisher)
            .receive_ros1_state(&mut ros1_state.published)
            .await;
        smth_changed |= self
            .bridges()
            .for_type(BridgeType::Service)
            .receive_ros1_state(&mut ros1_state.serviced)
            .await;
        smth_changed |= self
            .bridges()
            .for_type(BridgeType::Subscriber)
            .receive_ros1_state(&mut ros1_state.subscribed)
            .await;
        smth_changed
    }

    pub fn status(&self) -> BridgeStatus {
        self.bridges.status()
    }

    pub fn clear(&mut self) {
        self.bridges.clear()
    }
}
