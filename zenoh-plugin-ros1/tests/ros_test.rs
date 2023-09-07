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

use rosrust::{Publisher, RawMessage, Subscriber};
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    ros1_client::Ros1Client,
    test_helpers::{wait, BridgeChecker, IsolatedROSMaster, ROSEnvironment},
};

fn wait_for_rosclient_to_connect(rosclient: &Ros1Client) -> bool {
    async_std::task::block_on(wait(
        || rosclient.topic_types().is_ok(),
        Duration::from_secs(10),
    ))
}

fn wait_for_publishers(subscriber: &Subscriber, count: usize) -> bool {
    async_std::task::block_on(wait(
        || subscriber.publisher_count() == count,
        Duration::from_secs(10),
    ))
}

fn wait_for_subscribers(publisher: &Publisher<RawMessage>, count: usize) -> bool {
    async_std::task::block_on(wait(
        || publisher.subscriber_count() == count,
        Duration::from_secs(10),
    ))
}

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

    // check that they are isolated
    assert!(client.probe(Duration::from_secs(1)).is_ok());
    assert!(client.req(&RawMessage::default()).is_ok());

    // wait and check that they are still isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(client.probe(Duration::from_secs(1)).is_ok());
    assert!(client.req(&RawMessage::default()).is_ok());
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

    // check that they are isolated
    assert!(client.probe(Duration::from_secs(1)).is_ok());
    assert!(client.req(&RawMessage::default()).is_ok());

    // wait and check that they are still isolated
    std::thread::sleep(Duration::from_secs(5));
    assert!(client.probe(Duration::from_secs(1)).is_ok());
    assert!(client.req(&RawMessage::default()).is_ok());
}
