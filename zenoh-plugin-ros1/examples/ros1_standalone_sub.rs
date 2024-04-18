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

use zenoh_plugin_ros1::ros_to_zenoh_bridge::environment::Environment;

#[async_std::main]
async fn main() {
    let (sender, receiver) = unbounded();
    ctrlc::set_handler(move || {
        tracing::info!("Catching Ctrl+C...");
        sender
            .send_blocking(())
            .expect("Error handling Ctrl+C signal")
    })
    .expect("Error setting Ctrl+C handler");

    // initiate logging
    zenoh_util::try_init_log_from_env();

    // create ROS1 node and subscriber
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new_raw(
        Environment::ros_master_uri().get().as_str(),
        &rosrust::api::resolve::hostname(),
        &rosrust::api::resolve::namespace(),
        (Environment::ros_name().get() + "_test_subscriber_node").as_str(),
    )
    .expect("Error creating ROS1 Node!");
    println!(" OK!");
    print!("Creating ROS1 Subscriber...");
    #[allow(unused_variables)]
    let ros1_subscriber = ros1_node
        .subscribe("/some/ros/topic", 0, |msg: rosrust::RawMessage| {
            println!("ROS Subscriber: got message!")
        })
        .expect("Error creating ROS1 Subscriber!");
    println!(" OK!");

    println!("Running subscription, press Ctrl+C to exit...");
    // wait Ctrl+C
    receiver
        .recv()
        .await
        .expect("Error receiving Ctrl+C signal");
    tracing::info!("Caught Ctrl+C, stopping...");
}
