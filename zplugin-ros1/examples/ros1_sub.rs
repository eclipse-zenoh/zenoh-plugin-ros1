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

use zenoh_core::AsyncResolve;

use rosrust;
use zplugin_ros1::ros_to_zenoh_bridge::Ros1ToZenohBridge;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    // create bridge with ROS1 master
    // You need to have ros1 installed within your system and have "rosmaster" command available, otherwise this code will fail.
    // In this example the bridge will start ROS1 master by itself. 
    print!("Starting Bridge...");
    #[allow(unused_variables)]
    let bridge = Ros1ToZenohBridge::new_with_own_session(zenoh::config::default())
        .await
        .with_ros1_master()
        .await;
    println!(" OK!");

    // create ROS1 node and subscriber
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new("ROS1_test_node").unwrap();
    println!(" OK!");
    print!("Creating ROS1 Subscriber...");
    #[allow(unused_variables)]
    let ros1_subscriber = ros1_node
        .subscribe("/some/ros/topic/", 0, |msg: rosrust::RawMessage| {println!("ROS Subscriber: got message!")})
        .unwrap();
    println!(" OK!");

    // create Zenoh session and publisher
    print!("Creating Zenoh Session...");
    let zenoh_session = zenoh::open(zenoh::config::default())
        .res_async()
        .await
        .unwrap()
        .into_arc();
    println!(" OK!");
    print!("Creating Zenoh Publisher...");
    let zenoh_publisher = zenoh_session
        .declare_publisher("some/ros/topic")
        .congestion_control(zenoh::publication::CongestionControl::Block)
        .res_async()
        .await
        .unwrap();
    println!(" OK!");

    // here bridge will expose our test ROS topic to Zenoh so that our ROS1 subscriber will get data published in Zenoh
    println!("Running bridge, press Ctrl+C to exit...");

    // run test loop publishing data to Zenoh...
    let data: Vec<u8> = (0..10).collect();
    loop {
        println!("Zenoh Publisher: publishing data...");
        zenoh_publisher.put(data.clone()).res_async().await.unwrap();
        async_std::task::sleep(core::time::Duration::from_secs(1)).await;
    }
}
