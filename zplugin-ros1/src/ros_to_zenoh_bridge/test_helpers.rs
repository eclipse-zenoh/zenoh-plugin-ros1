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

use std::sync::atomic::Ordering::SeqCst;
use std::{net::SocketAddr, str::FromStr, sync::atomic::AtomicU16};
use zenoh::config::ModeDependentValue;

pub struct IsolatedPort {
    pub port: u16,
}
impl Default for IsolatedPort {
    fn default() -> Self {
        static TEST_PORT: AtomicU16 = AtomicU16::new(20000);
        Self {
            port: TEST_PORT.fetch_add(1, SeqCst),
        }
    }
}

#[derive(Default)]
pub struct IsolatedConfig {
    port: IsolatedPort,
}
impl IsolatedConfig {
    pub fn peer(&self) -> zenoh::config::Config {
        let mut config = zenoh::config::peer();
        config
            .scouting
            .multicast
            .set_address(Some(
                SocketAddr::from_str(format!("224.0.0.224:{}", self.port.port).as_str()).unwrap(),
            ))
            .unwrap();
        config
            .timestamping
            .set_enabled(Some(ModeDependentValue::Unique(true)))
            .unwrap();
        config
    }
}

#[derive(Default)]
pub struct IsolatedROSMaster {
    pub port: IsolatedPort,
}
impl IsolatedROSMaster {
    pub fn master_uri(&self) -> String {
        format!("http://localhost:{}/", self.port.port)
    }
}
