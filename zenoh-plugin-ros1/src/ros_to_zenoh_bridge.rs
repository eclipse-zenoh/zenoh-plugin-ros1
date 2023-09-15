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

use async_std::task::JoinHandle;

use log::error;
use zenoh;
use zenoh_core::{zresult::ZResult, AsyncResolve};

use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Arc,
};

use self::{environment::Environment, ros1_to_zenoh_bridge_impl::work_cycle};

#[cfg(feature = "test")]
pub mod aloha_declaration;
#[cfg(feature = "test")]
pub mod aloha_subscription;
#[cfg(feature = "test")]
pub mod bridge_type;
#[cfg(feature = "test")]
pub mod bridging_mode;
#[cfg(feature = "test")]
pub mod discovery;
#[cfg(feature = "test")]
pub mod ros1_client;
#[cfg(feature = "test")]
pub mod ros1_to_zenoh_bridge_impl;
#[cfg(feature = "test")]
pub mod rosclient_test_helpers;
#[cfg(feature = "test")]
pub mod service_cache;
#[cfg(feature = "test")]
pub mod test_helpers;
#[cfg(feature = "test")]
pub mod topic_utilities;
#[cfg(feature = "test")]
pub mod zenoh_client;

#[cfg(not(feature = "test"))]
mod aloha_declaration;
#[cfg(not(feature = "test"))]
mod aloha_subscription;
#[cfg(not(feature = "test"))]
mod bridge_type;
#[cfg(not(feature = "test"))]
mod bridging_mode;
#[cfg(not(feature = "test"))]
mod discovery;
#[cfg(not(feature = "test"))]
mod ros1_client;
#[cfg(not(feature = "test"))]
mod ros1_to_zenoh_bridge_impl;
#[cfg(not(feature = "test"))]
mod service_cache;
#[cfg(not(feature = "test"))]
mod topic_utilities;
#[cfg(not(feature = "test"))]
mod zenoh_client;

mod abstract_bridge;
mod bridges_storage;
mod topic_bridge;
mod topic_mapping;

pub mod environment;
pub mod ros1_master_ctrl;

pub struct Ros1ToZenohBridge {
    flag: Arc<AtomicBool>,
    task_handle: Box<JoinHandle<()>>,
}
impl Ros1ToZenohBridge {
    pub async fn new_with_own_session(config: zenoh::config::Config) -> ZResult<Self> {
        let session = zenoh::open(config).res_async().await?.into_arc();
        Ok(Self::new_with_external_session(session))
    }

    pub fn new_with_external_session(session: Arc<zenoh::Session>) -> Self {
        let flag = Arc::new(AtomicBool::new(true));
        Self {
            flag: flag.clone(),
            task_handle: Box::new(async_std::task::spawn(Self::run(session, flag))),
        }
    }

    pub async fn stop(&mut self) {
        self.flag.store(false, Relaxed);
        self.async_await().await;
    }

    //PRIVATE:
    async fn run(session: Arc<zenoh::Session>, flag: Arc<AtomicBool>) {
        if let Err(e) = work_cycle(
            Environment::ros_master_uri().get().as_str(),
            session,
            flag,
            |_v| {},
            |_status| {},
        )
        .await
        {
            error!("Error occured while running the bridge: {e}")
        }
    }

    async fn async_await(&mut self) {
        self.task_handle.as_mut().await;
    }
}
impl Drop for Ros1ToZenohBridge {
    fn drop(&mut self) {
        self.flag.store(false, Relaxed);
    }
}
