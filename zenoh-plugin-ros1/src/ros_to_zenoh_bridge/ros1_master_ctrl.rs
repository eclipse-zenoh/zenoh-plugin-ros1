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

use async_std::{
    process::{Child, Command},
    sync::Mutex,
};
use atoi::atoi;
use tracing::error;
use zenoh::{
    core::Result as ZResult,
    internal::{bail, zasynclock, zerror},
};

use crate::ros_to_zenoh_bridge::environment::Environment;

static ROSMASTER: Mutex<Option<Child>> = Mutex::new(None);

pub struct Ros1MasterCtrl;
impl Ros1MasterCtrl {
    pub async fn with_ros1_master() -> ZResult<()> {
        let uri = Environment::ros_master_uri().get();
        let splitted: Vec<&str> = uri.split(':').collect();
        if splitted.len() != 3 {
            bail!("Unable to parse port from ros_master_uri!")
        }
        let port_str = splitted[2].trim_matches(|v: char| !char::is_numeric(v));
        let port =
            atoi::<u16>(port_str.as_bytes()).ok_or("Unable to parse port from ros_master_uri!")?;

        let mut locked = zasynclock!(ROSMASTER);
        assert!(locked.is_none());
        let child = Command::new("rosmaster")
            .arg(format!("-p {}", port).as_str())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| zerror!("Error starting rosmaster process: {}", e))?;
        let _ = locked.insert(child);
        Ok(())
    }

    pub async fn without_ros1_master() {
        let mut locked = zasynclock!(ROSMASTER);
        assert!(locked.is_some());
        match locked.take() {
            Some(mut child) => match child.kill() {
                Ok(_) => {
                    if let Err(e) = child.status().await {
                        error!("Error stopping child rosmaster: {}", e);
                    }
                }
                Err(e) => error!("Error sending kill cmd to child rosmaster: {}", e),
            },
            None => match Command::new("killall").arg("rosmaster").spawn() {
                Ok(mut child) => {
                    if let Err(e) = child.status().await {
                        error!("Error stopping foreign rosmaster: {}", e);
                    }
                }
                Err(e) => error!(
                    "Error executing killall command to stop foreign rosmaster: {}",
                    e
                ),
            },
        }
    }
}
