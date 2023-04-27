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

use super::{
    abstract_bridge::AbstractBridge,
    bridge_type::BridgeType,
    discovery::{LocalResource, LocalResources},
    ros1_client, zenoh_client,
};
use log::error;
use std::sync::Arc;
use strum_macros::{Display, EnumString};

#[derive(PartialEq, Eq, EnumString, Clone, Display)]
#[strum(serialize_all = "snake_case")]
pub enum BridgingMode {
    Lazy,
    Auto,
}

pub struct TopicBridge {
    topic: rosrust::api::Topic,
    b_type: BridgeType,
    ros1_client: Arc<ros1_client::Ros1Client>,
    zenoh_client: Arc<zenoh_client::ZenohClient>,

    briging_mode: BridgingMode,
    required_on_ros1_side: bool,
    required_on_zenoh_side: bool,

    declaration_interface: Arc<LocalResources>,
    declaration: Option<LocalResource>,

    bridge: Option<AbstractBridge>,
}

impl TopicBridge {
    pub fn new(
        topic: rosrust::api::Topic,
        b_type: BridgeType,
        declaration_interface: Arc<LocalResources>,
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        briging_mode: BridgingMode,
    ) -> Self {
        Self {
            topic,
            b_type,
            ros1_client,
            zenoh_client,
            briging_mode,
            required_on_ros1_side: false,
            required_on_zenoh_side: false,
            declaration_interface,
            declaration: None,
            bridge: None,
        }
    }

    pub async fn set_present_in_ros1(&mut self, present: bool) -> bool {
        let recalc = self.required_on_ros1_side != present;
        self.required_on_ros1_side = present;
        if recalc {
            self.recalc_state().await;
        }
        recalc
    }

    pub async fn set_has_complementary_in_zenoh(&mut self, present: bool) -> bool {
        let recalc = self.required_on_zenoh_side != present;
        self.required_on_zenoh_side = present;
        if recalc {
            self.recalc_state().await;
        }
        recalc
    }

    pub fn is_bridging(&self) -> bool {
        self.bridge.is_some()
    }

    pub fn is_actual(&self) -> bool {
        match self.briging_mode {
            BridgingMode::Lazy => self.required_on_ros1_side || self.required_on_zenoh_side,
            BridgingMode::Auto => self.required_on_ros1_side,
        }
    }

    //PRIVATE:
    async fn recalc_state(&mut self) {
        self.recalc_declaration().await;
        self.recalc_bridging().await;
    }

    async fn recalc_declaration(&mut self) {
        match (self.required_on_ros1_side, &self.declaration) {
            (true, None) => {
                self.declaration = Some(
                    self.declaration_interface
                        .declare_with_type(&self.topic, self.b_type)
                        .await,
                );
            }
            (false, Some(_)) => {
                self.declaration = None;
            }
            (_, _) => {}
        }
    }

    async fn recalc_bridging(&mut self) {
        let is_discovered_client = self.b_type == BridgeType::Client && self.required_on_zenoh_side;
        let is_required = self.required_on_ros1_side
            && (self.briging_mode == BridgingMode::Auto || self.required_on_zenoh_side);

        if is_required || is_discovered_client {
            self.create_bridge().await;
        } else {
            self.bridge = None;
        }
    }

    async fn create_bridge(&mut self) {
        match AbstractBridge::new(
            self.b_type,
            &self.topic,
            &self.ros1_client,
            &self.zenoh_client,
        )
        .await
        {
            Ok(val) => {
                self.bridge = Some(val);
            }
            Err(e) => {
                self.bridge = None;
                error!("Error creating bridge: {}", e);
            }
        }
    }
}
