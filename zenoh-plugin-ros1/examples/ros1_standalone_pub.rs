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
        log::info!("Catching Ctrl+C...");
        sender
            .send_blocking(())
            .expect("Error handling Ctrl+C signal")
    })
    .expect("Error setting Ctrl+C handler");

    // initiate logging
    env_logger::init();

    // create ROS1 node and publisher
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new(
        (Environment::ros_name().get() + "_test_source_node").as_str(),
        Environment::ros_master_uri().get().as_str(),
    )
    .expect("Error creating ROS1 Node!");
    println!(" OK!");
    print!("Creating ROS1 Publisher...");
    let ros1_publisher = ros1_node
        .publish::<rosrust::RawMessage>("/some/ros/topic", 0)
        .expect("Error creating ROS1 Publisher!");
    println!(" OK!");

    println!("Running publication, press Ctrl+C to exit...");

    // run test loop publishing data to ROS topic...
    let working_loop = move || {
        let data: Vec<u8> = (0..10).collect();
        while receiver.try_recv().is_err() {
            println!("ROS Publisher: publishing data...");
            ros1_publisher
                .send(rosrust::RawMessage(data.clone()))
                .unwrap();
            std::thread::sleep(core::time::Duration::from_secs(1));
        }
        log::info!("Caught Ctrl+C, stopping...");
    };
    async_std::task::spawn_blocking(working_loop).await;
}
