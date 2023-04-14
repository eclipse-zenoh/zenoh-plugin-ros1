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

use zenoh::prelude::SplitBuffer;
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

    // create ROS1 node and service
    print!("Creating ROS1 Node...");
    let ros1_node = rosrust::api::Ros::new("ROS1_test_node").unwrap();
    println!(" OK!");
    print!("Creating ROS1 Service...");
    #[allow(unused_variables)]
    let ros1_service = ros1_node
        .service::<rosrust::RawMessage, _>("/some/ros/topic/", |query| -> rosrust::ServiceResult<rosrust::RawMessage> {println!("ROS Service: got query, sending reply..."); Ok(query) })
        .unwrap();
    println!(" OK!");

    // create Zenoh session
    print!("Creating Zenoh Session...");
    let zenoh_session = zenoh::open(zenoh::config::default())
        .res_async()
        .await
        .unwrap()
        .into_arc();
    println!(" OK!");

    // here bridge will expose our test ROS topic to Zenoh so that our ROS1 subscriber will get data published in Zenoh
    println!("Running bridge, press Ctrl+C to exit...");

    // run test loop querying Zenoh...
    let data: Vec<u8> = (0..10).collect();
    loop {
        println!("Zenoh: sending query..."); 
        let reply = zenoh_session.get("some/ros/topic").with_value(data.clone()).res_async().await.unwrap();
        let result = reply.recv_async().await;
        match result {
            Ok(val) => {
                println!("Zenoh: got reply!");
                assert!(data == val.sample.unwrap().value.payload.contiguous().to_vec() );
            }
            Err(_) => {}
        }
        async_std::task::sleep(core::time::Duration::from_secs(1)).await;
    }
}
