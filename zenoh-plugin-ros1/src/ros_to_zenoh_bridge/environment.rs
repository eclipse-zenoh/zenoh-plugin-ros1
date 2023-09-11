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

use duration_string::DurationString;
use log::error;
use rosrust::api::resolve::*;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::From;
use std::time::Duration;
use std::{marker::PhantomData, str::FromStr};

use super::bridging_mode::BridgingMode;

#[derive(Clone)]
pub struct Entry<'a, Tvar>
where
    Tvar: ToString + FromStr + Clone,
{
    pub name: &'a str,
    pub default: Tvar,
    marker: std::marker::PhantomData<Tvar>,
}
impl<'a, Tvar> Entry<'a, Tvar>
where
    Tvar: ToString + FromStr + Clone,
{
    fn new(name: &'a str, default: Tvar) -> Entry<'a, Tvar> {
        Entry {
            name,
            default,
            marker: PhantomData,
        }
    }

    pub fn get(&self) -> Tvar {
        if let Ok(val) = std::env::var(self.name) {
            if let Ok(val) = val.parse::<Tvar>() {
                return val;
            }
        }
        self.default.clone()
    }

    pub fn set(&self, value: Tvar) {
        std::env::set_var(self.name, value.to_string());
    }
}

impl<'a> From<Entry<'a, BridgingMode>> for Entry<'a, String> {
    fn from(item: Entry<'a, BridgingMode>) -> Entry<'a, String> {
        Entry::new(item.name, item.default.to_string())
    }
}

impl<'a> From<Entry<'a, DurationString>> for Entry<'a, String> {
    fn from(item: Entry<'a, DurationString>) -> Entry<'a, String> {
        Entry::new(item.name, item.default.to_string())
    }
}

impl<'a> From<Entry<'a, bool>> for Entry<'a, String> {
    fn from(item: Entry<'a, bool>) -> Entry<'a, String> {
        Entry::new(item.name, item.default.to_string())
    }
}

impl<'a> From<Entry<'a, CustomBridgingModes>> for Entry<'a, String> {
    fn from(item: Entry<'a, CustomBridgingModes>) -> Entry<'a, String> {
        Entry::new(item.name, item.default.to_string())
    }
}

#[derive(Clone, Default)]
pub struct CustomBridgingModes {
    pub modes: HashMap<String, BridgingMode>,
}

impl ToString for CustomBridgingModes {
    fn to_string(&self) -> String {
        let mut json_map = serde_json::Map::new();
        for (k, v) in &self.modes {
            json_map.insert(k.clone(), Value::String(v.to_string()));
        }
        serde_json::Value::Object(json_map).to_string()
    }
}

impl FromStr for CustomBridgingModes {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = CustomBridgingModes::default();
        let parsed: Value = serde_json::from_str(s)?;
        match parsed.as_object() {
            Some(v) => {
                for (topic, mode) in v {
                    match mode.as_str() {
                        Some(v) => match BridgingMode::from_str(v) {
                            Ok(mode) => {
                                result.modes.insert(topic.clone(), mode);
                            }
                            Err(e) => {
                                error!("Error reading value {v} as Bridging Mode: {e}");
                            }
                        },
                        None => {
                            error!("Error reading non-string as Bridging Mode");
                        }
                    }
                }
            }
            None => {
                error!("Error reading custom Bridging Mode map: the value provided is not a map!");
            }
        }

        Ok(result)
    }
}

pub struct Environment;
impl Environment {
    pub fn ros_master_uri() -> Entry<'static, String> {
        return Entry::new("ROS_MASTER_URI", master());
    }

    pub fn ros_hostname() -> Entry<'static, String> {
        return Entry::new("ROS_HOSTNAME", hostname());
    }

    pub fn ros_name() -> Entry<'static, String> {
        return Entry::new("ROS_NAME", name("ros1_to_zenoh_bridge"));
    }

    pub fn ros_namespace() -> Entry<'static, String> {
        return Entry::new("ROS_NAMESPACE", namespace());
    }

    pub fn with_rosmaster() -> Entry<'static, bool> {
        return Entry::new("WITH_ROSMASTER", false);
    }

    pub fn subscriber_bridging_mode() -> Entry<'static, BridgingMode> {
        return Entry::new("SUBSCRIBER_BRIDGING_MODE", BridgingMode::Auto);
    }
    pub fn subscriber_topic_custom_bridging_mode() -> Entry<'static, CustomBridgingModes> {
        return Entry::new(
            "SUBSCRIBER_TOPIC_CUSTOM_BRIDGING_MODE",
            CustomBridgingModes::default(),
        );
    }

    pub fn publisher_bridging_mode() -> Entry<'static, BridgingMode> {
        return Entry::new("PUBLISHER_BRIDGING_MODE", BridgingMode::Auto);
    }
    pub fn publisher_topic_custom_bridging_mode() -> Entry<'static, CustomBridgingModes> {
        return Entry::new(
            "PUBLISHER_TOPIC_CUSTOM_BRIDGING_MODE",
            CustomBridgingModes::default(),
        );
    }

    pub fn service_bridging_mode() -> Entry<'static, BridgingMode> {
        return Entry::new("SERVICE_BRIDGING_MODE", BridgingMode::Auto);
    }
    pub fn service_topic_custom_bridging_mode() -> Entry<'static, CustomBridgingModes> {
        return Entry::new(
            "SERVICE_TOPIC_CUSTOM_BRIDGING_MODE",
            CustomBridgingModes::default(),
        );
    }

    pub fn client_bridging_mode() -> Entry<'static, BridgingMode> {
        return Entry::new("CLIENT_BRIDGING_MODE", BridgingMode::Disabled);
    }
    pub fn client_topic_custom_bridging_mode() -> Entry<'static, CustomBridgingModes> {
        return Entry::new(
            "CLIENT_TOPIC_CUSTOM_BRIDGING_MODE",
            CustomBridgingModes::default(),
        );
    }

    pub fn master_polling_interval() -> Entry<'static, DurationString> {
        return Entry::new(
            "ROS_MASTER_POLLING_INTERVAL",
            DurationString::from(Duration::from_millis(100)),
        );
    }

    pub fn env() -> Vec<Entry<'static, String>> {
        return [
            Self::ros_master_uri(),
            Self::ros_hostname(),
            Self::ros_name(),
            Self::ros_namespace(),
            Self::subscriber_bridging_mode().into(),
            Self::publisher_bridging_mode().into(),
            Self::service_bridging_mode().into(),
            Self::client_bridging_mode().into(),
            Self::subscriber_topic_custom_bridging_mode().into(),
            Self::publisher_topic_custom_bridging_mode().into(),
            Self::service_topic_custom_bridging_mode().into(),
            Self::client_topic_custom_bridging_mode().into(),
            Self::master_polling_interval().into(),
            Self::with_rosmaster().into(),
        ]
        .to_vec();
    }
}
