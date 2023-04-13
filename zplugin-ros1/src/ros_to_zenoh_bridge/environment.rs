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

use std::str::FromStr;
use rosrust::api::resolve::*;

#[derive(Clone)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub default: String
}
impl<'a> Entry<'a> {
    fn new<Tvar>(name: &'a str, default: Tvar) -> Entry<'a>
    where
        Tvar: ToString
    {
        return Entry{name, default: default.to_string()};
    }

    pub fn get<Tvar>(&self) -> Tvar
    where
        Tvar: FromStr + std::convert::From<String>
    {
        match std::env::var(self.name) {
            Ok(val) => {
                match val.parse::<Tvar>() {
                    Ok(val) => return val,
                    Err(..) => {}
                };
            }
            Err(..) => {}
        };
        return self.default.clone().into();
    }

    pub fn set<Tvar>(&self, value: Tvar)
    where
        Tvar: ToString
    {
        std::env::set_var(self.name, value.to_string());
    }
}


pub struct Environment {}
impl Environment {
    pub fn ros_master_uri() -> Entry<'static> {
        return Entry::new("ROS_MASTER_URI", master());
    }

    pub fn ros_hostname() -> Entry<'static> {
        return Entry::new("ROS_HOSTNAME", hostname());
    }

    pub fn ros_name() -> Entry<'static> {
        return Entry::new("ROS_NAME", name("ros1_to_zenoh_bridge"));
    }

    pub fn ros_namespace() -> Entry<'static> {
        return Entry::new("ROS_NAMESPACE", namespace());
    }

    pub fn env() -> Vec<Entry<'static>> {
        return [
            Self::ros_master_uri(),
            Self::ros_hostname(),
            Self::ros_name(),
            Self::ros_namespace()
        ].to_vec();
    }
}