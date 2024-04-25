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

use ros_to_zenoh_bridge::environment::Environment;
use ros_to_zenoh_bridge::ros1_master_ctrl::Ros1MasterCtrl;
use ros_to_zenoh_bridge::Ros1ToZenohBridge;
use std::time::Duration;
use zenoh::plugins::{RunningPlugin, RunningPluginTrait, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh::Result as ZResult;
use zenoh_plugin_trait::{plugin_long_version, plugin_version, Plugin, PluginControl};

pub mod ros_to_zenoh_bridge;

// The struct implementing the ZenohPlugin and ZenohPlugin traits
pub struct Ros1Plugin {}

// declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
#[cfg(feature = "no_mangle")]
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
        zenoh_util::try_init_log_from_env();
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
            async_std::task::block_on(Ros1MasterCtrl::without_ros1_master());
        }
    }
}
impl Ros1PluginInstance {
    fn new(runtime: &Runtime) -> ZResult<Self> {
        let bridge: ZResult<Ros1ToZenohBridge> = async_std::task::block_on(async {
            if Environment::with_rosmaster().get() {
                Ros1MasterCtrl::with_ros1_master().await?;
                async_std::task::sleep(Duration::from_secs(1)).await;
            }

            // create a zenoh Session that shares the same Runtime as zenohd
            let session = zenoh::init(runtime.clone()).res().await?.into_arc();
            let bridge = ros_to_zenoh_bridge::Ros1ToZenohBridge::new_with_external_session(session);
            Ok(bridge)
        });

        Ok(Self { _bridge: bridge? })
    }
}
