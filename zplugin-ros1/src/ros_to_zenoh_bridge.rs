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

use async_std::{task::JoinHandle};

use zenoh_core::AsyncResolve;
use zenoh;

use std::{sync::{ 
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed}
}, process::Command};

use self::{ros1_to_zenoh_bridge_impl::work_cycle};

pub mod zenoh_client; 
pub mod ros1_client; 
pub mod discovery;
pub mod topic_bridge;
pub mod abstract_bridge;
pub mod bridge_type;

mod topic_utilities;
mod topic_mapping;
mod bridges_storage;

pub mod environment;
pub mod ros1_to_zenoh_bridge_impl;
pub mod aloha_declaration;
pub mod aloha_subscription;

pub struct Ros1ToZenohBridge {
    flag: Arc<AtomicBool>,
    task_handle: Box<JoinHandle<()>>,
    rosmaster: Option<std::process::Child>
}
impl Ros1ToZenohBridge {
    pub async fn new_with_own_session(config: zenoh::config::Config) -> Self {
        let session = zenoh::open(config).res_async().await.unwrap().into_arc();
        return Self::new_with_external_session(session).await;
    }

    pub async fn new_with_external_session(session: Arc<zenoh::Session>) -> Self {
        let flag = Arc::new(AtomicBool::new(true));
        Self {
            flag: flag.clone(),
            task_handle: Box::new(async_std::task::spawn(Self::run(session, flag))),
            rosmaster: None
        }
    }

    pub async fn with_ros1_master(mut self) -> Self {
        assert!(self.rosmaster.is_none());

        self.rosmaster = Some(
            Command::new("rosmaster")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );

        return self;
    }

    pub async fn without_ros1_master(mut self) -> Self {
        assert!(self.rosmaster.is_some());

        if self.rosmaster.is_some() {
            self.rosmaster.take().unwrap().kill().unwrap();
            self.rosmaster = None;
        }
        else {
            let mut kill_master = Command::new("killall").arg("rosmaster").spawn().unwrap();
            kill_master.wait().unwrap();
        }

        return self;
    }

    pub async fn async_await(&mut self) {
        self.task_handle.as_mut().await;
    }

    pub async fn stop(&mut self) {
        self.flag.store(false, Relaxed);
        self.async_await().await;
    }

    pub async fn run(
        session: Arc<zenoh::Session>,
        flag: Arc<AtomicBool>
    ) {
        work_cycle(
            session,
            flag,
            |_v| {},
            |_status| {}
        )
        .await;
    }
}
impl Drop for Ros1ToZenohBridge {
    fn drop(&mut self) {
        self.flag.store(false, Relaxed);
    }
}

