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
        .with_ros1_master();
    println!(" OK!");

    // create ROS1 node and publisher
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new("ROS1_test_node").unwrap();
    println!(" OK!");
    print!("Creating ROS1 Publisher...");
    let ros1_publisher = ros1_node
        .publish::<rosrust::RawMessage>("/some/ros/topic/", 0)
        .unwrap();
    println!(" OK!");

    // create Zenoh session and subscriber
    print!("Creating Zenoh Session...");
    let zenoh_session = zenoh::open(zenoh::config::default())
        .res_async()
        .await
        .unwrap()
        .into_arc();
    println!(" OK!");
    print!("Creating Zenoh Subscriber...");
    #[allow(unused_variables)]
    let zenoh_subscriber = zenoh_session
        .declare_subscriber("some/ros/topic")
        .callback(|data| println!("Zenoh Subscriber: got data!"))
        .res_async()
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
            ros1_publisher.send(rosrust::RawMessage(data.clone())).unwrap();
            std::thread::sleep(core::time::Duration::from_secs(1));
        }
    };
    async_std::task::spawn_blocking(working_loop).await;
}
