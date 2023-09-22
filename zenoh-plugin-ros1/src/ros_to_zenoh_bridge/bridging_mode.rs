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

use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};

use super::{bridge_type::BridgeType, environment::Environment};

#[derive(PartialEq, Eq, EnumString, Clone, Display)]
#[strum(serialize_all = "snake_case")]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum BridgingMode {
    LazyAuto,
    Auto,
    Disabled,
}

pub(super) fn bridging_mode(entity_type: BridgeType, topic_name: &str) -> BridgingMode {
    macro_rules! calcmode {
        ($custom_modes:expr, $default_mode:expr, $topic_name:expr) => {
            $custom_modes
                .get()
                .modes
                .get($topic_name)
                .map_or_else(|| $default_mode.get(), |v| v.clone())
        };
    }

    match entity_type {
        BridgeType::Publisher => {
            calcmode!(
                Environment::publisher_topic_custom_bridging_mode(),
                Environment::publisher_bridging_mode(),
                topic_name
            )
        }
        BridgeType::Subscriber => {
            calcmode!(
                Environment::subscriber_topic_custom_bridging_mode(),
                Environment::subscriber_bridging_mode(),
                topic_name
            )
        }
        BridgeType::Service => {
            calcmode!(
                Environment::service_topic_custom_bridging_mode(),
                Environment::service_bridging_mode(),
                topic_name
            )
        }
        BridgeType::Client => {
            calcmode!(
                Environment::client_topic_custom_bridging_mode(),
                Environment::client_bridging_mode(),
                topic_name
            )
        }
    }
}
