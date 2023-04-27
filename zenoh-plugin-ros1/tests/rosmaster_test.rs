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

#[test]
#[serial(ROS1)]
fn start_and_stop_master() {
    async_std::task::block_on(async {
        Ros1MasterCtrl::with_ros1_master()
            .await
            .expect("Error starting rosmaster");

        Ros1MasterCtrl::without_ros1_master().await;
    });
}

#[test]
#[serial(ROS1)]
fn start_and_stop_master_and_check_connectivity() {
    // start rosmaster
    async_std::task::block_on(async {
        Ros1MasterCtrl::with_ros1_master()
            .await
            .expect("Error starting rosmaster");
    });

    // start ros1 client
    let ros1_client = rosrust::api::Ros::new(
        "start_and_stop_master_and_check_connectivity_client",
        "http://localhost:11311/",
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
    async_std::task::block_on(async {
        Ros1MasterCtrl::without_ros1_master().await;
    });

    // check if there was a status from rosmaster...
    if !has_rosmaster {
        panic!("cannot see rosmaster!");
    }
}
