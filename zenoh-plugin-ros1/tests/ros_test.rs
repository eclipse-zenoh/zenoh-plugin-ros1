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

use rosrust::{RawMessage, RawMessageDescription};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    ros1_client::Ros1Client,
    rosclient_test_helpers::{
        wait_for_publishers, wait_for_rosclient_to_connect, wait_for_subscribers,
    },
    test_helpers::{BridgeChecker, IsolatedROSMaster, ROSEnvironment},
};

#[test]
fn check_rosclient_connectivity_ros_then_client() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient = Ros1Client::new("test_client", &roscfg.master_uri()).unwrap();
    assert!(wait_for_rosclient_to_connect(&rosclient));
}

#[test]
fn check_rosclient_connectivity_client_then_ros() {
    // reserve isolated ros config
    let roscfg = IsolatedROSMaster::default();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");

    // start rosmaster
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // check that the client is connected
    assert!(wait_for_rosclient_to_connect(&rosclient));
}

#[test]
fn check_rosclient_many_pub_single_sub_operation() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let rosclient_pub1 = Ros1Client::new("test_client_pub1", &roscfg.master_uri())
        .expect("error creating rosclient_pub1!");
    let rosclient_pub2 = Ros1Client::new("test_client_pub2", &roscfg.master_uri())
        .expect("error creating rosclient_pub2!");
    let rosclient_sub = Ros1Client::new("test_client_sub", &roscfg.master_uri())
        .expect("error creating rosclient_sub!");

    assert!(wait_for_rosclient_to_connect(&rosclient_pub1));
    assert!(wait_for_rosclient_to_connect(&rosclient_pub2));
    assert!(wait_for_rosclient_to_connect(&rosclient_sub));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_many_pub_single_sub_operation");

    // create two publishers and one subscriber
    let publisher1 = rosclient_pub1
        .publish(&shared_topic)
        .expect("error creating publisher1!");
    let publisher2 = rosclient_pub2
        .publish(&shared_topic)
        .expect("error creating publisher2!");
    let subscriber = rosclient_sub
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber!");

    // check that they are connected
    assert!(wait_for_subscribers(&publisher1, 1));
    assert!(wait_for_subscribers(&publisher2, 1));
    assert!(wait_for_publishers(&subscriber, 2));

    // wait and check that they are still connected
    std::thread::sleep(Duration::from_secs(5));
    assert!(wait_for_subscribers(&publisher1, 1));
    assert!(wait_for_subscribers(&publisher2, 1));
    assert!(wait_for_publishers(&subscriber, 2));
}

#[test]
fn check_rosclient_single_pub_many_sub_operation() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let rosclient_pub = Ros1Client::new("test_client_pub", &roscfg.master_uri())
        .expect("error creating rosclient_pub!");
    let rosclient_sub1 = Ros1Client::new("test_client_sub1", &roscfg.master_uri())
        .expect("error creating rosclient_sub1!");
    let rosclient_sub2 = Ros1Client::new("test_client_sub2", &roscfg.master_uri())
        .expect("error creating rosclient_sub2!");

    assert!(wait_for_rosclient_to_connect(&rosclient_pub));
    assert!(wait_for_rosclient_to_connect(&rosclient_sub1));
    assert!(wait_for_rosclient_to_connect(&rosclient_sub2));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_single_pub_many_sub_operation");

    // create two publishers and one subscriber
    let publisher = rosclient_pub
        .publish(&shared_topic)
        .expect("error creating publisher!");
    let subscriber1 = rosclient_sub1
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber1!");
    let subscriber2 = rosclient_sub2
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber2!");

    // check that they are connected
    assert!(wait_for_publishers(&subscriber1, 1));
    assert!(wait_for_publishers(&subscriber2, 1));
    assert!(wait_for_subscribers(&publisher, 2));

    // wait and check that they are still connected
    std::thread::sleep(Duration::from_secs(5));
    assert!(wait_for_publishers(&subscriber1, 1));
    assert!(wait_for_publishers(&subscriber2, 1));
    assert!(wait_for_subscribers(&publisher, 2));
}

#[test]
fn check_rosclient_pub_sub_connection_pub_then_sub() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let rosclient1 =
        Ros1Client::new("test_client1", &roscfg.master_uri()).expect("error creating Ros1Client1!");
    let rosclient2 =
        Ros1Client::new("test_client2", &roscfg.master_uri()).expect("error creating Ros1Client2!");
    assert!(wait_for_rosclient_to_connect(&rosclient1));
    assert!(wait_for_rosclient_to_connect(&rosclient2));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_pub_sub_connection");

    // create publisher and subscriber
    let publisher = rosclient1
        .publish(&shared_topic)
        .expect("error creating publisher!");
    let subscriber = rosclient2
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber!");

    // check that they are connected
    assert!(wait_for_publishers(&subscriber, 1));
    assert!(wait_for_subscribers(&publisher, 1));

    // wait and check that they are still connected
    std::thread::sleep(Duration::from_secs(5));
    assert!(subscriber.publisher_count() == 1);
    assert!(publisher.subscriber_count() == 1);
}

#[test]
fn check_rosclient_pub_sub_connection_sub_then_pub() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let rosclient1 =
        Ros1Client::new("test_client1", &roscfg.master_uri()).expect("error creating Ros1Client1!");
    let rosclient2 =
        Ros1Client::new("test_client2", &roscfg.master_uri()).expect("error creating Ros1Client2!");
    assert!(wait_for_rosclient_to_connect(&rosclient1));
    assert!(wait_for_rosclient_to_connect(&rosclient2));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_pub_sub_connection");

    // create subscriber and publisher
    let subscriber = rosclient2
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber!");
    let publisher = rosclient1
        .publish(&shared_topic)
        .expect("error creating publisher!");

    // check that they are connected
    assert!(wait_for_publishers(&subscriber, 1));
    assert!(wait_for_subscribers(&publisher, 1));

    // wait and check that they are still connected
    std::thread::sleep(Duration::from_secs(5));
    assert!(subscriber.publisher_count() == 1);
    assert!(publisher.subscriber_count() == 1);
}

#[test]
fn check_rosclient_pub_sub_isolation_pub_then_sub() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_pub_sub_isolation");

    // create publisher and subscriber
    let publisher = rosclient
        .publish(&shared_topic)
        .expect("error creating publisher!");
    let subscriber = rosclient
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber!");

    // check that they are isolated
    assert!(subscriber.publisher_count() == 0);
    assert!(publisher.subscriber_count() == 0);

    // wait and check that they are still isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(subscriber.publisher_count() == 0);
    assert!(publisher.subscriber_count() == 0);
}

#[test]
fn check_rosclient_pub_sub_isolation_sub_then_pub() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_rosclient_pub_sub_isolation");

    // create publisher and subscriber
    let subscriber = rosclient
        .subscribe(&shared_topic, |_: RawMessage| {})
        .expect("error creating subscriber!");
    let publisher = rosclient
        .publish(&shared_topic)
        .expect("error creating publisher!");

    // check that they are isolated
    assert!(subscriber.publisher_count() == 0);
    assert!(publisher.subscriber_count() == 0);

    // wait and check that they are still isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(subscriber.publisher_count() == 0);
    assert!(publisher.subscriber_count() == 0);
}

#[test]
fn prove_rosclient_service_non_isolation_service_then_client() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("prove_rosclient_service_non_isolation");

    // create service and client
    let _service = rosclient
        .service::<rosrust::RawMessage, _>(&shared_topic, Ok)
        .expect("error creating service!");
    let client = rosclient
        .client(&shared_topic)
        .expect("error creating client!");

    // datatype extended description
    let description = RawMessageDescription {
        msg_definition: "*".to_string(),
        md5sum: shared_topic.md5.clone(),
        msg_type: shared_topic.datatype.clone(),
    };

    // check that they are not isolated
    assert!(client
        .probe_with_description(Duration::from_secs(1), description.clone())
        .is_ok());
    assert!(client
        .req_with_description(&RawMessage::default(), description.clone())
        .is_ok());

    // wait and check that they are still not isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(client
        .probe_with_description(Duration::from_secs(1), description.clone())
        .is_ok());
    assert!(client
        .req_with_description(&RawMessage::default(), description.clone())
        .is_ok());
}

#[test]
fn prove_rosclient_service_non_isolation_client_then_service() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("prove_rosclient_service_non_isolation");

    // create client and service
    let client = rosclient
        .client(&shared_topic)
        .expect("error creating client!");
    let _service = rosclient
        .service::<rosrust::RawMessage, _>(&shared_topic, Ok)
        .expect("error creating service!");

    // datatype extended description
    let description = RawMessageDescription {
        msg_definition: "*".to_string(),
        md5sum: shared_topic.md5.clone(),
        msg_type: shared_topic.datatype.clone(),
    };

    // check that they are not isolated
    assert!(client
        .probe_with_description(Duration::from_secs(1), description.clone())
        .is_ok());
    assert!(client
        .req_with_description(&RawMessage::default(), description.clone())
        .is_ok());

    // wait and check that they are still not isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(client
        .probe_with_description(Duration::from_secs(1), description.clone())
        .is_ok());
    assert!(client
        .req_with_description(&RawMessage::default(), description.clone())
        .is_ok());
}

#[test]
fn service_preserve_datatype_incorrect_datatype() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let rosclient =
        Ros1Client::new("test_client", &roscfg.master_uri()).expect("error creating Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("prove_rosclient_service_non_isolation");

    // create client and service
    let client = rosclient
        .client(&shared_topic)
        .expect("error creating client!");
    let _service = rosclient
        .service::<rosrust::RawMessage, _>(&shared_topic, Ok)
        .expect("error creating service!");

    // datatype extended description
    let description = RawMessageDescription {
        msg_definition: "*".to_string(),
        md5sum: "deadbeeef".to_string(), // only this makes the request to fail
        msg_type: "incorrect/datatype".to_string(),
    };

    // check that datatype is preserved
    assert!(client
        .req_with_description(&RawMessage::default(), description)
        .is_err());
}
