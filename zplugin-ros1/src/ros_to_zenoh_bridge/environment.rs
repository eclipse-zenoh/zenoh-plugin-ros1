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
use rosrust::api::resolve::*;
use std::convert::From;
use std::time::Duration;
use std::{marker::PhantomData, str::FromStr};

use super::topic_bridge::BridgingMode;

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

    pub fn bridging_mode() -> Entry<'static, BridgingMode> {
        return Entry::new("ROS_BRIDGING_MODE", BridgingMode::Automatic);
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
            Self::bridging_mode().into(),
            Self::master_polling_interval().into(),
        ]
        .to_vec();
    }
}
