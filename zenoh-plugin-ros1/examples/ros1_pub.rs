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

use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    environment::Environment, ros1_master_ctrl::Ros1MasterCtrl, Ros1ToZenohBridge,
};

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::try_init_log_from_env();

    // You need to have ros1 installed within your system and have "rosmaster" command available, otherwise this code will fail.
    // start ROS1 master...
    print!("Starting ROS1 Master...");
    Ros1MasterCtrl::with_ros1_master()
        .await
        .expect("Error starting rosmaster!");

    // create bridge
    print!("Starting Bridge...");
    let _bridge = Ros1ToZenohBridge::new_with_own_session(zenoh::config::peer()).await;
    println!(" OK!");

    // create ROS1 node and publisher
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new_raw(
        Environment::ros_master_uri().get().as_str(),
        &rosrust::api::resolve::hostname(),
        &rosrust::api::resolve::namespace(),
        (Environment::ros_name().get() + "_test_source_node").as_str(),
    )
    .unwrap();
    println!(" OK!");
    print!("Creating ROS1 Publisher...");
    let ros1_publisher = ros1_node
        .publish::<rosrust::RawMessage>("/some/ros/topic", 0)
        .unwrap();
    println!(" OK!");

    // create Zenoh session and subscriber
    print!("Creating Zenoh Session...");
    let zenoh_session = zenoh::open(zenoh::config::default()).await.unwrap();
    println!(" OK!");
    print!("Creating Zenoh Subscriber...");
    #[allow(unused_variables)]
    let zenoh_subscriber = zenoh_session
        .declare_subscriber("some/ros/topic")
        .callback(|data| println!("Zenoh Subscriber: got data!"))
        .await
        .unwrap();
    println!(" OK!");

    // here bridge will expose our test ROS topic to Zenoh so that our subscriber will get data published within it
    println!("Running bridge, press Ctrl+C to exit...");

    // run test loop publishing data to ROS topic...
    let working_loop = move || {
        let data: Vec<u8> = (0..10).collect();
        loop {
            println!("ROS Publisher: publishing data...");
            ros1_publisher
                .send(rosrust::RawMessage(data.clone()))
                .unwrap();
            std::thread::sleep(core::time::Duration::from_secs(1));
        }
    };
    tokio::task::spawn_blocking(working_loop).await.unwrap();
}
