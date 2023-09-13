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

use rosrust::api::Topic;
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    ros1_client::Ros1Client,
    rosclient_test_helpers::wait_for_rosclient_to_connect,
    service_cache::Ros1ServiceCache,
    test_helpers::{BridgeChecker, IsolatedROSMaster, ROSEnvironment},
};

#[test]
fn check_service_cache_single_node() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclient
    let service_node_name = "test_service_node";
    let service_rosclient = Ros1Client::new(service_node_name, &roscfg.master_uri())
        .expect("error creating service Ros1Client!");
    let watching_rosclient = Ros1Client::new("watching_node", &roscfg.master_uri())
        .expect("error creating watching Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&service_rosclient));
    assert!(wait_for_rosclient_to_connect(&watching_rosclient));

    // create shared topic
    let shared_topic = BridgeChecker::make_topic("check_service_cache_single_node");

    // create service
    let _service = service_rosclient
        .service::<rosrust::RawMessage, _>(&shared_topic, Ok)
        .expect("error creating service!");

    // create cache
    let mut cache = Ros1ServiceCache::new("ros_service_cache_node", &roscfg.master_uri())
        .expect("error creating Ros1ServiceCache");

    // check that there is a proper service with one proper node in rosmaster's system state
    let state = watching_rosclient
        .state()
        .expect("error getting ROS state!");

    assert_eq!(state.services.len(), 1);
    assert_eq!(state.services[0].connections.len(), 1);
    assert_eq!(
        state.services[0].connections[0],
        format!("/{service_node_name}")
    );

    // resolve datatype
    let resolved_datatype = cache
        .resolve_datatype(shared_topic.name, service_node_name.to_string())
        .unwrap();

    assert_eq!(resolved_datatype, shared_topic.datatype);
}

#[test]
fn check_service_cache_single_node_many_resolutions() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let service_node_name = "test_service_node";
    let service_rosclient = Ros1Client::new(service_node_name, &roscfg.master_uri())
        .expect("error creating service Ros1Client!");
    let watching_rosclient = Ros1Client::new("watching_node", &roscfg.master_uri())
        .expect("error creating watching Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&service_rosclient));
    assert!(wait_for_rosclient_to_connect(&watching_rosclient));

    // create shared topic
    let shared_topic =
        BridgeChecker::make_topic("check_service_cache_single_node_many_resolutions");

    // create service
    let _service = service_rosclient
        .service::<rosrust::RawMessage, _>(&shared_topic, Ok)
        .expect("error creating service!");

    // create cache
    let mut cache = Ros1ServiceCache::new("ros_service_cache_node", &roscfg.master_uri())
        .expect("error creating Ros1ServiceCache");

    // check that there is a proper service with one proper node in rosmaster's system state
    let state = watching_rosclient
        .state()
        .expect("error getting ROS state!");

    assert_eq!(state.services.len(), 1);
    assert_eq!(state.services[0].connections.len(), 1);
    assert_eq!(
        state.services[0].connections[0],
        format!("/{service_node_name}")
    );

    // resolve datatype many times
    for _ in 0..10 {
        let resolved_datatype = cache
            .resolve_datatype(shared_topic.name.clone(), service_node_name.to_string())
            .unwrap();
        assert_eq!(resolved_datatype, shared_topic.datatype);
    }
}

#[test]
fn check_service_cache_many_nodes() {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let service_node_name1 = "test_service_node1";
    let service_node_name2 = "test_service_node2";
    let service_rosclient1 = Ros1Client::new(service_node_name1, &roscfg.master_uri())
        .expect("error creating service Ros1Client 1!");
    let service_rosclient2 = Ros1Client::new(service_node_name2, &roscfg.master_uri())
        .expect("error creating service Ros1Client 2!");
    let watching_rosclient = Ros1Client::new("watching_node", &roscfg.master_uri())
        .expect("error creating watching Ros1Client!");
    assert!(wait_for_rosclient_to_connect(&service_rosclient1));
    assert!(wait_for_rosclient_to_connect(&service_rosclient2));
    assert!(wait_for_rosclient_to_connect(&watching_rosclient));

    // create shared topics
    let shared_topic1 = BridgeChecker::make_topic("check_service_cache_many_nodes");
    let shared_topic2 = Topic {
        name: shared_topic1.name.clone(),
        datatype: "some/other".to_string(),
    };

    // create services
    let _service1 = service_rosclient1
        .service::<rosrust::RawMessage, _>(&shared_topic1, Ok)
        .expect("error creating service 1!");
    let _service2 = service_rosclient2
        .service::<rosrust::RawMessage, _>(&shared_topic2, Ok)
        .expect("error creating service 2!");

    // create cache
    let mut cache = Ros1ServiceCache::new("ros_service_cache_node", &roscfg.master_uri())
        .expect("error creating Ros1ServiceCache");

    // check that there is a proper service with one proper node in rosmaster's system state
    // note: currently, ROS1 intends to have not more than one node for each service topic,
    // so for current test case service2 should hide service1
    let state = watching_rosclient
        .state()
        .expect("error getting ROS state!");
    assert_eq!(state.services.len(), 1);
    assert_eq!(state.services[0].connections.len(), 1);
    assert_eq!(
        state.services[0].connections[0],
        format!("/{service_node_name2}")
    );

    // resolve datatype for service 1
    let resolved_datatype = cache
        .resolve_datatype(shared_topic1.name, service_node_name1.to_string())
        .unwrap();
    assert_eq!(resolved_datatype, shared_topic2.datatype);

    // resolve datatype for service 2
    let resolved_datatype = cache
        .resolve_datatype(shared_topic2.name, service_node_name2.to_string())
        .unwrap();
    assert_eq!(resolved_datatype, shared_topic2.datatype);
}
