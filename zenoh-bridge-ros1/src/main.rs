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
use async_std::channel::unbounded;
use clap::{App, Arg};
use std::str::FromStr;
use zenoh::config::Config;
use zenoh::prelude::*;
use zenoh_plugin_ros1::ros_to_zenoh_bridge::environment::Environment;

macro_rules! insert_json5 {
    ($config: expr, $args: expr, $key: expr, if $name: expr) => {
        if $args.occurrences_of($name) > 0 {
            $config.insert_json5($key, "true").unwrap();
        }
    };
    ($config: expr, $args: expr, $key: expr, if $name: expr, $($t: tt)*) => {
        if $args.occurrences_of($name) > 0 {
            $config.insert_json5($key, &serde_json::to_string(&$args.value_of($name).unwrap()$($t)*).unwrap()).unwrap();
        }
    };
    ($config: expr, $args: expr, $key: expr, for $name: expr, $($t: tt)*) => {
        if let Some(value) = $args.values_of($name) {
            $config.insert_json5($key, &serde_json::to_string(&value$($t)*).unwrap()).unwrap();
        }
    };
}

fn parse_args() -> Config {
    let app = App::new("zenoh bridge for ROS1")
        .version(zenoh_plugin_ros1::GIT_VERSION)
        .long_version(zenoh_plugin_ros1::LONG_VERSION.as_str())
        //
        // zenoh related arguments:
        //
        .arg(Arg::from_usage(
r#"-i, --id=[HEX_STRING] \
'The identifier (as an hexadecimal string, with odd number of chars - e.g.: 0A0B23...) that zenohd must use.
WARNING: this identifier must be unique in the system and must be 16 bytes maximum (32 chars)!
If not set, a random UUIDv4 will be used.'"#,
            ))
        .arg(Arg::from_usage(
r#"-m, --mode=[MODE]  'The zenoh session mode.'"#)
            .possible_values(["peer", "client"])
            .default_value("peer")
        )
        .arg(Arg::from_usage(
r#"-c, --config=[FILE] \
'The configuration file. Currently, this file must be a valid JSON5 file.'"#,
            ))
        .arg(Arg::from_usage(
r#"-l, --listen=[ENDPOINT]... \
'A locator on which this router will listen for incoming sessions.
Repeat this option to open several listeners.'"#,
                ),
            )
        .arg(Arg::from_usage(
r#"-e, --connect=[ENDPOINT]... \
'A peer locator this router will try to connect to.
Repeat this option to connect to several peers.'"#,
            ))
        .arg(Arg::from_usage(
r#"--no-multicast-scouting \
'By default the zenoh bridge listens and replies to UDP multicast scouting messages for being discovered by peers and routers.
This option disables this feature.'"#
        ))
        .arg(Arg::from_usage(
r#"--rest-http-port=[PORT | IP:PORT] \
'Configures HTTP interface for the REST API (disabled by default, setting this option enables it). Accepted values:'
  - a port number
  - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)."#
        ))
        //
        // ros1 related arguments:
        //
        .arg(Arg::from_usage(
r#"--ros_master_uri=[ENDPOINT] \
'A URI of the ROS1 Master to connect to, the defailt is http://localhost:11311/'"#
        ))
        .arg(Arg::from_usage(
r#"--ros_hostname=[String]   'A hostname to send to ROS1 Master, the default is system's hostname'"#
        ))
        .arg(Arg::from_usage(
r#"--ros_name=[String]   'A bridge node's name for ROS1, the default is "ros1_to_zenoh_bridge"'"#
        ))
        .arg(Arg::from_usage(
r#"--subscriber_bridging_mode=[String] \
'Mode of subscriber's topic bridging. Accepted values:'
  - "auto"(default) - bridge topics once they are declared locally or discovered remotely
  - "lazy_auto" - bridge topics once they are both declared locally and discovered remotely
  - "disabled" - never bridge topics. This setting will also suppress the topic discovery."#
        ))
        .arg(Arg::from_usage(
r#"--publisher_bridging_mode=[String] \
'Mode of publisher's topic bridging. Accepted values:'
  - "auto"(default) - bridge topics once they are declared locally or discovered remotely
  - "lazy_auto" - bridge topics once they are both declared locally and discovered remotely
  - "disabled" - never bridge topics. This setting will also suppress the topic discovery."#
        ))
        .arg(Arg::from_usage(
r#"--service_bridging_mode=[String] \
'Mode of service's topic bridging. Accepted values:'
  - "auto"(default) - bridge topics once they are declared locally or discovered remotely
  - "lazy_auto" - bridge topics once they are both declared locally and discovered remotely
  - "disabled" - never bridge topics. This setting will also suppress the topic discovery."#
        ))
        .arg(Arg::from_usage(
r#"--client_bridging_mode=[String] \
'Mode of client's topic bridging. Accepted values:'
  - "auto" - bridge topics once they are discovered remotely
  - "disabled"(default) - never bridge topics. This setting will also suppress the topic discovery.
  NOTE: there are some pecularities on how ROS1 handles clients:
  - ROS1 doesn't provide any client discovery mechanism
  - ROS1 doesn't allow multiple services on the same topic
  Due to this, client's bridging works differently compared to pub\sub bridging:
  - lazy bridging mode is not available as there is no way to discover local ROS1 clients
  - client bridging is disabled by default, as it may brake the local ROS1 system if it intends to have client and service interacting on the same topic  
  In order to use client bridging, you have two options:
  - globally select auto bridging mode (with caution!) with this option
  - bridge specific topics using 'client_topic_custom_bridging_mode' option (with a little bit less caution!)"#
        ))

        .arg(Arg::from_usage(
r#"--ros_master_polling_interval=[String] \
'An interval to poll the ROS1 master for status
Bridge polls ROS1 master to get information on local topics. This option is the interval of this polling. The default is "100ms".
Accepted value:'
A string such as 100ms, 2s, 5m
The string format is [0-9]+(ns|us|ms|[smhdwy])"#
        ))
        .arg(Arg::from_usage(
r#"--with_rosmaster=[bool]   'An option wether the bridge should run it's own rosmaster process, the default is "false"'"#
        ));
    let args = app.get_matches();

    // load config file at first
    let mut config = match args.value_of("config") {
        Some(conf_file) => Config::from_file(conf_file).unwrap(),
        None => Config::default(),
    };
    // if "ros1" plugin conf is not present, add it (empty to use default config)
    if config.plugin("ros1").is_none() {
        config.insert_json5("plugins/ros1", "{}").unwrap();
    }

    // apply zenoh related arguments over config
    // NOTE: only if args.occurrences_of()>0 to avoid overriding config with the default arg value
    if args.occurrences_of("id") > 0 {
        config
            .set_id(ZenohId::from_str(args.value_of("id").unwrap()).unwrap())
            .unwrap();
    }
    if args.occurrences_of("mode") > 0 {
        config
            .set_mode(Some(args.value_of("mode").unwrap().parse().unwrap()))
            .unwrap();
    }
    if let Some(endpoints) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(endpoints.map(|p| p.parse().unwrap()))
    }
    if let Some(endpoints) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(endpoints.map(|p| p.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }
    if let Some(port) = args.value_of("rest-http-port") {
        config
            .insert_json5("plugins/rest/http_port", &format!(r#""{port}""#))
            .unwrap();
    }

    // apply ros1 related arguments over config
    // run through the bridge's supported config options and fill them from command line options
    let plugin_configuration_entries = Environment::env();
    for entry in plugin_configuration_entries.iter() {
        let lowercase_name = entry.name.to_lowercase();
        let lowercase_path = format!("plugins/ros1/{}", lowercase_name);
        insert_json5!(config, args, lowercase_path.as_str(), if lowercase_name.as_str(),);
    }

    config
}

#[async_std::main]
async fn main() {
    let (sender, receiver) = unbounded();
    ctrlc::set_handler(move || {
        sender
            .send_blocking(())
            .expect("Error handling Ctrl+C signal")
    })
    .expect("Error setting Ctrl+C handler");

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("z=info")).init();
    log::info!("zenoh-bridge-ros1 {}", *zenoh_plugin_ros1::LONG_VERSION);

    let config = parse_args();

    // create a zenoh Runtime (to share with plugins)
    let runtime = zenoh::runtime::Runtime::new(config)
        .await
        .expect("Error creating runtime");

    // start ros1 plugin
    use zenoh_plugin_trait::Plugin;
    let _running_bridge = zenoh_plugin_ros1::Ros1Plugin::start("ros1", &runtime)
        .expect("Error starting zenoh-bridge-ros1");

    // wait Ctrl+C
    receiver
        .recv()
        .await
        .expect("Error receiving Ctrl+C signal");
    log::info!("Caught Ctrl+C, stopping bridge...");
}
