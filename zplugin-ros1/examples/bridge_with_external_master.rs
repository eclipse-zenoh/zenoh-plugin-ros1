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

use zplugin_ros1::ros_to_zenoh_bridge::Ros1ToZenohBridge;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    // create bridge with ROS1 master
    // In this example the bridge will connect to master specified by ROS_MASTER_URI env variable (default http://localhost:11311/)
    print!("Starting Bridge...");
    #[allow(unused_variables)]
    let bridge = Ros1ToZenohBridge::new_with_own_session(zenoh::config::default()).await;
    println!(" OK!");

    println!("Running bridge, press Ctrl+C to exit...");
    async_std::task::sleep(core::time::Duration::MAX).await;
}
