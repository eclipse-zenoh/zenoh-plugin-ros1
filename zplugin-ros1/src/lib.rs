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
use ros_to_zenoh_bridge::Ros1ToZenohBridge;
use std::sync::Arc;
use zenoh::plugins::{Plugin, RunningPluginTrait, ValidationFunction, ZenohPlugin};
use zenoh::prelude::r#async::*;
use zenoh::runtime::Runtime;
use zenoh_core::{bail, Result as ZResult};

pub mod ros_to_zenoh_bridge;

// The struct implementing the ZenohPlugin and ZenohPlugin traits
pub struct Ros1Plugin {}

// declaration of the plugin's VTable for zenohd to find the plugin's functions to be called
zenoh_plugin_trait::declare_plugin!(Ros1Plugin);

impl ZenohPlugin for Ros1Plugin {}
impl Plugin for Ros1Plugin {
    type StartArgs = Runtime;
    type RunningPlugin = zenoh::plugins::RunningPlugin;

    // A mandatory const to define, in case of the plugin is built as a standalone executable
    const STATIC_NAME: &'static str = "ros1";

    // The first operation called by zenohd on the plugin
    fn start(name: &str, runtime: &Self::StartArgs) -> ZResult<Self::RunningPlugin> {
        let config = runtime.config.lock();
        let self_cfg = config
            .plugin(name)
            .ok_or("No plugin in the config!")?
            .as_object()
            .ok_or("Unable to get cfg objet!")?;

        // run through the bridge's config options and fill them from plugins config
        let plugin_configuration_entries = Environment::env();
        for entry in plugin_configuration_entries.iter() {
            if let Some(v) = self_cfg.get(entry.name) {
                entry.set(v);
            }
        }

        std::mem::drop(config);

        // return a RunningPlugin to zenohd
        Ok(Box::new(RunningPlugin::new(runtime)?))
    }
}

// The RunningPlugin struct implementing the RunningPluginTrait trait
struct RunningPlugin {
    _bridge: Option<Ros1ToZenohBridge>,
}
impl RunningPluginTrait for RunningPlugin {
    // Operation returning a ValidationFunction(path, old, new)-> ZResult<Option<serde_json::Map<String, serde_json::Value>>>
    // this function will be called each time the plugin's config is changed via the zenohd admin space
    fn config_checker(&self) -> ValidationFunction {
        Arc::new(move |_path, _old, _new| {
            bail!("Reconfiguration at runtime is not allowed!");
        })
    }

    // Function called on any query on admin space that matches this plugin's sub-part of the admin space.
    // Thus the plugin can reply its contribution to the global admin space of this zenohd.
    fn adminspace_getter<'a>(
        &'a self,
        _selector: &'a Selector<'a>,
        _plugin_status_key: &str,
    ) -> ZResult<Vec<zenoh::plugins::Response>> {
        Ok(Vec::new())
    }
}

impl RunningPlugin {
    fn new(runtime: &Runtime) -> ZResult<Self> {
        let bridge: ZResult<Ros1ToZenohBridge> = async_std::task::block_on(async {
            // create a zenoh Session that shares the same Runtime than zenohd
            let session = zenoh::init(runtime.clone()).res().await?.into_arc();
            let bridge = ros_to_zenoh_bridge::Ros1ToZenohBridge::new_with_external_session(session);
            Ok(bridge)
        });

        Ok(Self {
            _bridge: Some(bridge?),
        })
    }
}
