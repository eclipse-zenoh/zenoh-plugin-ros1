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

use async_std::{sync::{Mutex, MutexGuard}};
use futures::{FutureExt, select, pin_mut};

use zenoh;

use std::{sync::{
    Arc,
    atomic::{AtomicBool, Ordering::Relaxed}
}};

use log::{debug, error};

use crate::ros_to_zenoh_bridge::{bridges_storage::BridgesStorage, topic_mapping, zenoh_client, ros1_client, discovery::LocalResources, environment::Environment};

use super::{discovery::DiscoveryBuilder, ros1_client::Ros1Client};



#[derive(PartialEq, Clone, Copy)]
pub enum RosStatus {
    Unknown,
    Synchronizing,
    Ok,
    Error
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub struct BridgeStatus {
    pub ros_publishers: (usize, usize),
    pub ros_subscribers: (usize, usize),
    pub ros_services: (usize, usize),
    pub ros_clients: (usize, usize)
}

pub async fn work_cycle<RosStatusCallback,
                        BridgeStatisticsCallback>(session: Arc<zenoh::Session>, 
                                                  flag: Arc<AtomicBool>,
                                                  ros_status_callback: RosStatusCallback,
                                                  statistics_callback: BridgeStatisticsCallback)
where
    RosStatusCallback: Fn(RosStatus),
    BridgeStatisticsCallback: Fn(BridgeStatus)

{
    let ros1_client = Arc::new(ros1_client::Ros1Client::new(Environment::ros_name().get::<String>().as_str() ));
    let zenoh_client = Arc::new(zenoh_client::ZenohClient::new(session.clone()));

    let local_resources = Arc::new(LocalResources::new(
        "*".to_string(),
        "*".to_string(),
        session.clone()));

    let bridges = Arc::new(Mutex::new(BridgesStorage::new(
        ros1_client.clone(),
        zenoh_client,
        local_resources
    )));


    let mut bridge = RosToZenohBridge::new(ros_status_callback, 
                                                                                   statistics_callback);

    let discovery = make_discovery(session.clone(), bridges.clone()).fuse();

    let run = bridge.run(
        ros1_client,
        bridges,
        flag
    ).fuse();

    pin_mut!(discovery, run);

    select! {
        _ = discovery => {},
        _ = run => {}
    };

}

async fn make_discovery<'a>(session: Arc<zenoh::Session>,
                            bridges: Arc<Mutex<BridgesStorage>>) {
    let mut builder = DiscoveryBuilder::new(
        "*".to_string(),
        "*".to_string(),
        session); 
    
    let (add_tx, add_rx) = flume::unbounded();
    let (rem_tx, rem_rx) = flume::unbounded();

    let add_tx = Arc::new(add_tx);
    let rem_tx = Arc::new(rem_tx);

    builder
    .on_discovered(move |b_type, topic| {
        let add_tx = add_tx.clone();
        Box::new( Box::pin(
            async move {
                match add_tx.send_async((b_type, topic)).await {
                    Ok(_) => {},
                    Err(e) => {
                        error!("Error posting topic discoery to channel: {}", e);
                    }
                }
            }
        ))
    })
    .on_lost(move |b_type, topic|{
        let rem_tx = rem_tx.clone();
        Box::new( Box::pin(
            async move {
                match rem_tx.send_async((b_type, topic)).await {
                    Ok(_) => {},
                    Err(e) => {
                        error!("Error posting topic loss to channel: {}", e);
                    }
                }
            }
        ))
    }); 

    let _discovery = builder.build().await;   

    loop {
        select! {
            add = add_rx.recv_async().fuse() => {
                match add {
                    Ok((b_type, topic)) => { 
                        bridges.lock().await.bridges().complementary_for(b_type).complementary_entity_discovered(topic).await;
                    }
                    Err(_) => todo!(),
                }
            }
            rem = rem_rx.recv_async().fuse() => {
                match rem {
                    Ok((b_type, topic)) => { 
                        bridges.lock().await.bridges().complementary_for(b_type).complementary_entity_lost(topic).await;
                    }
                    Err(_) => todo!(),
                }
            }
        }
    } 
}



struct RosToZenohBridge<RosStatusCallback,
                        BridgeStatisticsCallback>
where
    RosStatusCallback: Fn(RosStatus),
    BridgeStatisticsCallback: Fn(BridgeStatus)
{
    ros_status: RosStatus,
    ros_status_callback: RosStatusCallback,

    statistics_callback: BridgeStatisticsCallback
}

impl<RosStatusCallback,
     BridgeStatisticsCallback> RosToZenohBridge<RosStatusCallback,
                                                BridgeStatisticsCallback>
where
    RosStatusCallback: Fn(RosStatus),
    BridgeStatisticsCallback: Fn(BridgeStatus)
{
// PUBLIC
    pub fn new(ros_status_callback: RosStatusCallback,
               statistics_callback: BridgeStatisticsCallback) -> Self {
        RosToZenohBridge {
            ros_status: RosStatus::Unknown,
            ros_status_callback,

            statistics_callback
        }
    }

    pub async fn run(&mut self,
                     ros1_client: Arc<Ros1Client>,
                     bridges: Arc<Mutex<BridgesStorage>>,
                     flag: Arc<AtomicBool>) {
        while flag.load(Relaxed) {
            let cl = ros1_client.clone();
            let ros1_state = async_std::task::spawn_blocking(
                move || { topic_mapping::Ros1TopicMapping::topic_mapping(cl.as_ref()) }
            ).await;

            debug!("ros state: {:#?}", ros1_state);

            let mut locked = bridges.lock().await;
            match ros1_state {
                Ok(mut ros1_state_val) => {
                    self.transit_ros_status(RosStatus::Ok);

                    if locked.receive_ros1_state(&mut ros1_state_val).await {
                        self.report_bridge_statistics(&locked).await;
                        async_std::task::sleep(core::time::Duration::from_millis(100)).await;
                    }
                    else {
                        self.report_bridge_statistics(&locked).await;
                        async_std::task::sleep(core::time::Duration::from_millis(200)).await;
                    }
                }
                Err(e) => {
                    error!("Error reading ROS state: {}", e);

                    self.transit_ros_status(RosStatus::Error);
                    Self::cleanup(&mut locked);
                    self.report_bridge_statistics(&locked).await;

                    async_std::task::sleep(core::time::Duration::from_millis(500)).await;
                }
            }
        }
    }

// PRIVATE
    fn transit_ros_status(&mut self, new_ros_status: RosStatus) {
        if self.ros_status != new_ros_status {
            self.ros_status = new_ros_status;
            (self.ros_status_callback)(self.ros_status);
        }
    }

    async fn report_bridge_statistics(&self, locked: &MutexGuard<'_, BridgesStorage>) {
        (self.statistics_callback)(locked.status());
    }

    fn cleanup(locked: &mut MutexGuard<'_, BridgesStorage>) {
        locked.clear();
    }
}
