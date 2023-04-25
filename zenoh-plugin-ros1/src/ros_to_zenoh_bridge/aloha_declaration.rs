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

use zenoh::buffers::ZBuf;

use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;

use zenoh::prelude::r#async::*;
use zenoh::Session;

pub struct AlohaDeclaration {
    monitor_running: Arc<AtomicBool>,
}
impl Drop for AlohaDeclaration {
    fn drop(&mut self) {
        self.monitor_running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
impl AlohaDeclaration {
    pub fn new(session: Arc<Session>, key: OwnedKeyExpr, beacon_period: Duration) -> Self {
        let monitor_running = Arc::new(AtomicBool::new(true));
        async_std::task::spawn(Self::aloha_monitor_task(
            beacon_period,
            monitor_running.clone(),
            key,
            session,
        ));
        Self { monitor_running }
    }

    //PRIVATE:
    async fn aloha_monitor_task(
        beacon_period: Duration,
        monitor_running: Arc<AtomicBool>,
        key: OwnedKeyExpr,
        session: Arc<Session>,
    ) {
        let beacon_task_flag = Arc::new(AtomicBool::new(false));

        let remote_beacons = Arc::new(AtomicUsize::new(0));
        let rb = remote_beacons.clone();
        let _beacon_listener = session
            .declare_subscriber(key.clone())
            .allowed_origin(Locality::Remote)
            .reliability(Reliability::BestEffort)
            .callback(move |_| {
                rb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .res_async()
            .await
            .unwrap();

        let mut sending_beacons = true;
        Self::start_beacon_task(
            beacon_period,
            key.clone(),
            session.clone(),
            beacon_task_flag.clone(),
        );

        while monitor_running.load(std::sync::atomic::Ordering::Relaxed) {
            match remote_beacons.fetch_and(0, std::sync::atomic::Ordering::SeqCst) {
                0 => {
                    if !sending_beacons {
                        // start publisher in ALOHA style...
                        let period_ns = beacon_period.as_nanos();
                        let aloha_wait: u128 = rand::random::<u128>() % period_ns;
                        async_std::task::sleep(Duration::from_nanos(
                            aloha_wait.try_into().unwrap(),
                        ))
                        .await;
                        if remote_beacons.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                            Self::start_beacon_task(
                                beacon_period,
                                key.clone(),
                                session.clone(),
                                beacon_task_flag.clone(),
                            );
                            sending_beacons = true;
                        }
                    }
                }
                _ => {
                    if sending_beacons && rand::random::<bool>() {
                        Self::stop_beacon_task(beacon_task_flag.clone());
                        sending_beacons = false;
                    }
                }
            }
            async_std::task::sleep(beacon_period).await;
        }
        Self::stop_beacon_task(beacon_task_flag.clone());
    }

    fn start_beacon_task(
        beacon_period: Duration,
        key: OwnedKeyExpr,
        session: Arc<Session>,
        running: Arc<AtomicBool>,
    ) {
        running.store(true, std::sync::atomic::Ordering::SeqCst);
        async_std::task::spawn(Self::aloha_publishing_task(
            beacon_period,
            key,
            session,
            running,
        ));
    }

    fn stop_beacon_task(running: Arc<AtomicBool>) {
        running.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    async fn aloha_publishing_task(
        beacon_period: Duration,
        key: OwnedKeyExpr,
        session: Arc<Session>,
        running: Arc<AtomicBool>,
    ) {
        let publisher = session
            .declare_publisher(key)
            .allowed_destination(Locality::Remote)
            .congestion_control(CongestionControl::Drop)
            .priority(Priority::Background)
            .res_async()
            .await
            .unwrap();

        while running.load(std::sync::atomic::Ordering::Relaxed) {
            let _res = publisher
                .put(zenoh::value::Value::new(ZBuf::default()))
                .res_async()
                .await;
            async_std::task::sleep(beacon_period).await;
        }
    }
}
