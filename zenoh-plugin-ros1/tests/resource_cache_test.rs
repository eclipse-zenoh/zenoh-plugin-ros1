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

use rosrust::RawMessage;
use zenoh_plugin_ros1::ros_to_zenoh_bridge::{
    resource_cache::Ros1ResourceCache,
    ros1_client::Ros1Client,
    rosclient_test_helpers::wait_for_rosclient_to_connect,
    test_helpers::{BridgeChecker, IsolatedROSMaster, ROSEnvironment},
    topic_descriptor::TopicDescriptor,
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
    let mut cache = Ros1ResourceCache::new(
        "ros_service_cache_node",
        String::default(),
        &roscfg.master_uri(),
    )
    .expect("error creating Ros1ResourceCache");

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
    let (resolved_datatype, resolved_md5) = cache
        .resolve_service_parameters(shared_topic.name, service_node_name.to_string())
        .unwrap();

    assert_eq!(resolved_datatype, shared_topic.datatype);
    assert_eq!(resolved_md5, shared_topic.md5);
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
    let mut cache = Ros1ResourceCache::new(
        "ros_service_cache_node",
        String::default(),
        &roscfg.master_uri(),
    )
    .expect("error creating Ros1ResourceCache");

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
        let (resolved_datatype, resolved_md5) = cache
            .resolve_service_parameters(shared_topic.name.clone(), service_node_name.to_string())
            .unwrap();
        assert_eq!(resolved_datatype, shared_topic.datatype);
        assert_eq!(resolved_md5, shared_topic.md5);
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
    let shared_topic2 = TopicDescriptor {
        name: shared_topic1.name.clone(),
        datatype: "some/other".to_string(),
        md5: "other".to_string(),
    };

    // create services
    let _service1 = service_rosclient1
        .service::<rosrust::RawMessage, _>(&shared_topic1, Ok)
        .expect("error creating service 1!");
    let _service2 = service_rosclient2
        .service::<rosrust::RawMessage, _>(&shared_topic2, Ok)
        .expect("error creating service 2!");

    // create cache
    let mut cache = Ros1ResourceCache::new(
        "ros_service_cache_node",
        String::default(),
        &roscfg.master_uri(),
    )
    .expect("error creating Ros1ResourceCache");

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
    let (resolved_datatype, resolved_md5) = cache
        .resolve_service_parameters(shared_topic1.name, service_node_name1.to_string())
        .unwrap();
    assert_eq!(resolved_datatype, shared_topic2.datatype);
    assert_eq!(resolved_md5, shared_topic2.md5);

    // resolve datatype for service 2
    let (resolved_datatype, resolved_md5) = cache
        .resolve_service_parameters(shared_topic2.name, service_node_name2.to_string())
        .unwrap();
    assert_eq!(resolved_datatype, shared_topic2.datatype);
    assert_eq!(resolved_md5, shared_topic2.md5);
}

fn start_nodes_with_environment(
    node_count: i32,
) -> (
    ROSEnvironment,
    IsolatedROSMaster,
    Vec<(i32, String, Ros1Client)>,
    TopicDescriptor,
) {
    // init and start isolated ros
    let roscfg = IsolatedROSMaster::default();
    let _ros_env = ROSEnvironment::new(roscfg.port.port).with_master();

    // start rosclients
    let nodes: Vec<_> = (0..node_count)
        .map(|n| {
            let node_name = format!("test_client_node_{n}");
            let rosclient = Ros1Client::new(&node_name, &roscfg.master_uri())
                .expect("error creating Ros1Client!");
            (n, rosclient.ros.name().to_owned(), rosclient)
        })
        .collect();

    // wait for all rosclients to connect
    for (_n, _nodename, rosclient) in &nodes {
        assert!(wait_for_rosclient_to_connect(rosclient));
    }

    // create topic
    let topic = TopicDescriptor {
        name: "/mytopic".to_string(),
        datatype: "std_msgs/String".to_string(),
        md5: "somemd5".to_string(),
    };

    (_ros_env, roscfg, nodes, topic)
}

fn publisher_cache_test(nodes: i32) {
    // start ROS environment, test nodes, create topic
    let (_ros_env, roscfg, nodes, topic) = start_nodes_with_environment(nodes);

    // publish topic
    let publishers: Vec<_> = nodes
        .iter()
        .map(|(n, nodename, rosclient)| {
            let particular_topic = TopicDescriptor {
                name: topic.name.clone(),
                datatype: format!("{}{n}", topic.datatype),
                md5: format!("{}{n}", topic.md5),
            };
            let publisher = rosclient
                .publish(&particular_topic)
                .expect("error creating publisher!");
            (particular_topic, nodename, publisher, rosclient)
        })
        .collect();

    // create cache
    let mut cache = Ros1ResourceCache::new(
        "ros_service_cache_node",
        String::default(),
        &roscfg.master_uri(),
    )
    .expect("error creating Ros1ResourceCache");

    // check all resolutions
    for (particular_topic, node_name, _publisher, _rosclient) in publishers {
        // resolve topic parameters
        let (resolved_datatype, resolved_md5) = cache
            .resolve_publisher_parameters(particular_topic.name.clone(), node_name.to_owned())
            .unwrap();

        // validate the parameters
        assert_eq!(resolved_datatype, particular_topic.datatype);
        assert_eq!(resolved_md5, particular_topic.md5);
    }
}

#[test]
fn publisher_cache_test_single_node() {
    publisher_cache_test(1);
}

#[test]
fn publisher_cache_test_many_nodes() {
    publisher_cache_test(10);
}

fn subscriber_cache_test(nodes: i32, cache_before_subscribers: bool) {
    // start ROS environment, test nodes, create topic
    let (_ros_env, roscfg, nodes, topic) = start_nodes_with_environment(nodes);

    let mut cache = Ros1ResourceCache::new(
        "ros_service_cache_node",
        String::default(),
        &roscfg.master_uri(),
    )
    .expect("error creating Ros1ResourceCache");

    let subscribers: Vec<_> = nodes
        .iter()
        .map(|(n, node_name, rosclient)| {
            let particular_topic = TopicDescriptor {
                name: topic.name.clone(),
                datatype: format!("{}{n}", topic.datatype),
                md5: format!("{}{n}", topic.md5),
            };
            // the Ros1ResourceCache is lazy, so in order to start caching subscriptions to particular topic
            // we need to try with resolve_subscriber_parameters call here
            if cache_before_subscribers {
                let _ = cache.resolve_subscriber_parameters(
                    particular_topic.name.clone(),
                    node_name.to_owned(),
                );
            }
            let subscriber = rosclient
                .subscribe(&particular_topic, |_: RawMessage| {})
                .expect("error creating subscriber!");
            (particular_topic, node_name, subscriber, rosclient)
        })
        .collect();

    // check all resolutions
    for (particular_topic, node_name, _subscriber, _rosclient) in subscribers {
        // resolve topic parameters from multiple tries
        for n in 0..100 {
            assert!(n < 99);
            if let Ok((resolved_datatype, resolved_md5)) = cache
                .resolve_subscriber_parameters(particular_topic.name.clone(), node_name.to_owned())
            {
                assert_eq!(resolved_datatype, particular_topic.datatype);
                assert_eq!(resolved_md5, particular_topic.md5);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn subscriber_cache_test_single_node_cache_before_subs() {
    subscriber_cache_test(1, true);
}

#[test]
fn subscriber_cache_test_many_nodes_cache_before_subs() {
    subscriber_cache_test(10, true);
}

#[test]
fn subscriber_cache_test_single_node_cache_after_subs() {
    subscriber_cache_test(1, false);
}

#[test]
fn subscriber_cache_test_many_nodes_cache_after_subs() {
    subscriber_cache_test(10, false);
}
