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
use log::error;
use zenoh::plugins::ZResult;
use zenoh_core::bail;

static ROSMASTER: Mutex<Option<Child>> = Mutex::new(None);

pub struct Ros1MasterCtrl;
impl Ros1MasterCtrl {
    pub async fn with_ros1_master() -> ZResult<()> {
        let mut locked = ROSMASTER.lock().await;
        assert!(locked.is_none());
        match Command::new("rosmaster")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                let _ = locked.insert(child);
                Ok(())
            }
            Err(e) => {
                bail!("Error starting rosmaster: {}", e);
            }
        }
    }

    pub async fn without_ros1_master() {
        let mut locked = ROSMASTER.lock().await;
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
