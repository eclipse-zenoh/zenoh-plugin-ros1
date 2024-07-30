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

use std::time::Duration;

use serial_test::serial;
use zenoh_plugin_ros1::ros_to_zenoh_bridge::ros1_master_ctrl::Ros1MasterCtrl;

#[tokio::test(flavor = "multi_thread")]
#[serial(ROS1)]
async fn start_and_stop_master() {
    Ros1MasterCtrl::with_ros1_master()
        .await
        .expect("Error starting rosmaster");

    Ros1MasterCtrl::without_ros1_master().await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial(ROS1)]
async fn start_and_stop_master_and_check_connectivity() {
    // start rosmaster
    Ros1MasterCtrl::with_ros1_master()
        .await
        .expect("Error starting rosmaster");

    // start ros1 client
    let ros1_client = rosrust::api::Ros::new_raw(
        "http://localhost:11311/",
        &rosrust::api::resolve::hostname(),
        &rosrust::api::resolve::namespace(),
        "start_and_stop_master_and_check_connectivity_client",
    )
    .unwrap();

    // wait for correct  status from rosmaster....
    let mut has_rosmaster = false;
    for _i in 0..1000 {
        if ros1_client.state().is_ok() {
            has_rosmaster = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // stop rosmaster
    Ros1MasterCtrl::without_ros1_master().await;

    // check if there was a status from rosmaster...
    if !has_rosmaster {
        panic!("cannot see rosmaster!");
    }
}
