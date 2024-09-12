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
#![recursion_limit = "1024"]

use std::{
    future::Future,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use ros_to_zenoh_bridge::{
    environment::Environment, ros1_master_ctrl::Ros1MasterCtrl, Ros1ToZenohBridge,
};
use tokio::task::JoinHandle;
use zenoh::{
    internal::{
        plugins::{RunningPlugin, RunningPluginTrait, ZenohPlugin},
        runtime::Runtime,
    },
    Result as ZResult,
};
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};

use crate::ros_to_zenoh_bridge::environment;

pub mod ros_to_zenoh_bridge;

lazy_static::lazy_static! {
    static ref WORK_THREAD_NUM: AtomicUsize = AtomicUsize::new(environment::DEFAULT_WORK_THREAD_NUM);
    static ref MAX_BLOCK_THREAD_NUM: AtomicUsize = AtomicUsize::new(environment::DEFAULT_MAX_BLOCK_THREAD_NUM);
    // The global runtime is used in the dynamic plugins, which we can't get the current runtime
    static ref TOKIO_RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
               .worker_threads(WORK_THREAD_NUM.load(Ordering::SeqCst))
               .max_blocking_threads(MAX_BLOCK_THREAD_NUM.load(Ordering::SeqCst))
               .enable_all()
               .build()
               .expect("Unable to create runtime");
}
#[inline(always)]
pub(crate) fn spawn_blocking_runtime<F, R>(func: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), use the current runtime
            rt.spawn_blocking(func)
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), reuse the global runtime
            TOKIO_RUNTIME.spawn_blocking(func)
        }
    }
}
#[inline(always)]
pub(crate) fn spawn_runtime<F>(task: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), use the current runtime
            rt.spawn(task)
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), reuse the global runtime
            TOKIO_RUNTIME.spawn(task)
        }
    }
}
#[inline(always)]
pub(crate) fn blockon_runtime<F: Future>(task: F) -> F::Output {
    // Check whether able to get the current runtime
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            // Able to get the current runtime (standalone binary), use the current runtime
            tokio::task::block_in_place(|| rt.block_on(task))
        }
        Err(_) => {
            // Unable to get the current runtime (dynamic plugins), reuse the global runtime
            tokio::task::block_in_place(|| TOKIO_RUNTIME.block_on(task))
        }
    }
}

// The struct implementing the ZenohPlugin and ZenohPlugin traits
pub struct Ros1Plugin {}

// declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
#[cfg(feature = "dynamic_plugin")]
zenoh_plugin_trait::declare_plugin!(Ros1Plugin);

impl ZenohPlugin for Ros1Plugin {}
impl Plugin for Ros1Plugin {
    type StartArgs = Runtime;
    type Instance = RunningPlugin;

    // A mandatory const to define, in case of the plugin is built as a standalone executable
    const DEFAULT_NAME: &'static str = "ros1";
    const PLUGIN_VERSION: &'static str = plugin_version!();
    const PLUGIN_LONG_VERSION: &'static str = plugin_long_version!();

    // The first operation called by zenohd on the plugin
    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::Instance> {
        // Try to initiate login.
        // Required in case of dynamic lib, otherwise no logs.
        // But cannot be done twice in case of static link.
        zenoh::try_init_log_from_env();
        tracing::debug!("ROS1 plugin {}", Ros1Plugin::PLUGIN_LONG_VERSION);

        let config = runtime.config().lock();
        let self_cfg = config
            .plugin(name)
            .ok_or("No plugin in the config!")?
            .as_object()
            .ok_or("Unable to get cfg objet!")?;
        tracing::info!("ROS1 config: {:?}", self_cfg);

        // run through the bridge's config options and fill them from plugins config
        let plugin_configuration_entries = Environment::env();
        for entry in plugin_configuration_entries.iter() {
            if let Some(v) = self_cfg.get(&entry.name.to_lowercase()) {
                let str = v.to_string();
                entry.set(str.trim_matches('"').to_string());
            }
        }
        // Setup the thread numbers
        WORK_THREAD_NUM.store(Environment::work_thread_num().get(), Ordering::SeqCst);
        MAX_BLOCK_THREAD_NUM.store(Environment::max_block_thread_num().get(), Ordering::SeqCst);

        drop(config);

        // return a RunningPlugin to zenohd
        Ok(Box::new(Ros1PluginInstance::new(runtime)?))
    }
}

// The RunningPlugin struct implementing the RunningPluginTrait trait
struct Ros1PluginInstance {
    _bridge: Ros1ToZenohBridge,
}
impl PluginControl for Ros1PluginInstance {}
impl RunningPluginTrait for Ros1PluginInstance {}
impl Drop for Ros1PluginInstance {
    fn drop(&mut self) {
        if Environment::with_rosmaster().get() {
            blockon_runtime(Ros1MasterCtrl::without_ros1_master());
        }
    }
}
impl Ros1PluginInstance {
    fn new(runtime: &Runtime) -> ZResult<Self> {
        let bridge: ZResult<Ros1ToZenohBridge> = blockon_runtime(async {
            if Environment::with_rosmaster().get() {
                Ros1MasterCtrl::with_ros1_master().await?;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            // create a zenoh Session that shares the same Runtime as zenohd
            let session = zenoh::session::init(runtime.clone()).await?;
            let bridge = ros_to_zenoh_bridge::Ros1ToZenohBridge::new_with_external_session(session);
            Ok(bridge)
        });

        Ok(Self { _bridge: bridge? })
    }
}
