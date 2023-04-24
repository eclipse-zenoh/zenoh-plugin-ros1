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

use std::future;

use zplugin_ros1::ros_to_zenoh_bridge::{ros1_master_ctrl::Ros1MasterCtrl, Ros1ToZenohBridge};

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    // You need to have ros1 installed within your system and have "rosmaster" command available, otherwise this code will fail.
    // start ROS1 master...
    print!("Starting ROS1 Master...");
    Ros1MasterCtrl::with_ros1_master()
        .await
        .expect("Error starting rosmaster!");

    // create bridge
    print!("Starting Bridge...");
    let _bridge = Ros1ToZenohBridge::new_with_own_session(zenoh::config::default()).await;
    println!(" OK!");

    println!("Running bridge, press Ctrl+C to exit...");
    future::pending::<()>().await;
}
