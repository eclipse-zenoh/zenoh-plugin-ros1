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

use std::{fmt::Display, sync::Arc};

use tracing::error;

use super::{
    abstract_bridge::AbstractBridge,
    bridge_type::BridgeType,
    bridging_mode::BridgingMode,
    discovery::{LocalResource, LocalResources},
    ros1_client,
    topic_descriptor::TopicDescriptor,
    zenoh_client,
};

pub struct TopicBridge {
    topic: TopicDescriptor,
    b_type: BridgeType,
    ros1_client: Arc<ros1_client::Ros1Client>,
    zenoh_client: Arc<zenoh_client::ZenohClient>,

    bridging_mode: BridgingMode,
    required_on_ros1_side: bool,
    required_on_zenoh_side: bool,

    declaration_interface: Arc<LocalResources>,
    declaration: Option<LocalResource>,

    bridge: Option<AbstractBridge>,
}

impl Display for TopicBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:?}", self.b_type, self.topic)
    }
}

impl TopicBridge {
    pub fn new(
        topic: TopicDescriptor,
        b_type: BridgeType,
        declaration_interface: Arc<LocalResources>,
        ros1_client: Arc<ros1_client::Ros1Client>,
        zenoh_client: Arc<zenoh_client::ZenohClient>,
        bridging_mode: BridgingMode,
    ) -> Self {
        Self {
            topic,
            b_type,
            ros1_client,
            zenoh_client,
            bridging_mode,
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
        self.required_on_ros1_side || self.required_on_zenoh_side
    }

    //PRIVATE:
    async fn recalc_state(&mut self) {
        self.recalc_declaration().await;
        self.recalc_bridging().await;
    }

    async fn recalc_declaration(&mut self) {
        match (self.required_on_ros1_side, &self.declaration) {
            (true, None) => {
                match self
                    .declaration_interface
                    .declare_with_type(&self.topic, self.b_type)
                    .await
                {
                    Ok(decl) => self.declaration = Some(decl),
                    Err(e) => error!("{self}: error declaring discovery: {e}"),
                }
            }
            (false, Some(_)) => {
                self.declaration = None;
            }
            (_, _) => {}
        }
    }

    async fn recalc_bridging(&mut self) {
        let is_discovered_client = self.b_type == BridgeType::Client && self.required_on_zenoh_side;

        let is_required = is_discovered_client
            || match (self.required_on_zenoh_side, self.required_on_ros1_side) {
                (true, true) => true,
                (false, false) => false,
                (_, _) => self.bridging_mode == BridgingMode::Auto,
            };

        match (is_required, self.bridge.as_ref()) {
            (true, Some(_)) => {}
            (true, None) => self.create_bridge().await,
            (false, _) => self.bridge = None,
        }
    }

    async fn create_bridge(&mut self) {
        match AbstractBridge::new(
            self.b_type,
            &self.topic,
            &self.ros1_client,
            &self.zenoh_client,
            &self.declaration_interface.bridge_namespace,
        )
        .await
        {
            Ok(val) => {
                self.bridge = Some(val);
            }
            Err(e) => {
                self.bridge = None;
                error!("{self}: error creating bridge: {e}");
            }
        }
    }
}
