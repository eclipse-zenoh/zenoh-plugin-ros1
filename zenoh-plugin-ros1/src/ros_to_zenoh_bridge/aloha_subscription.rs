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

use std::{
    collections::{hash_map::Entry::*, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering::Relaxed},
        Arc,
    },
    time::Duration,
};

use flume::Receiver;
use futures::{join, Future, FutureExt};
use tokio::sync::Mutex;
use tracing::error;
use zenoh::{key_expr::OwnedKeyExpr, prelude::*, sample::Sample, Result as ZResult, Session};

use crate::spawn_runtime;

struct AlohaResource {
    activity: AtomicBool,
}
impl AlohaResource {
    fn new() -> Self {
        Self {
            activity: AtomicBool::new(true),
        }
    }

    pub fn update(&mut self) {
        self.activity.store(true, Relaxed);
    }

    pub fn reset(&mut self) {
        self.activity.store(false, Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.activity.load(Relaxed)
    }
}

pub type TCallback = dyn Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output = ()> + Unpin + Send>
    + Send
    + Sync
    + 'static;

pub struct AlohaSubscription {
    task_running: Arc<AtomicBool>,
}

impl Drop for AlohaSubscription {
    fn drop(&mut self) {
        self.task_running.store(false, Relaxed);
    }
}
impl AlohaSubscription {
    pub async fn new<F>(
        session: Arc<Session>,
        key: OwnedKeyExpr,
        beacon_period: Duration,
        on_resource_declared: F,
        on_resource_undeclared: F,
    ) -> ZResult<Self>
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        let task_running = Arc::new(AtomicBool::new(true));

        spawn_runtime(AlohaSubscription::task(
            task_running.clone(),
            key,
            beacon_period,
            session,
            on_resource_declared,
            on_resource_undeclared,
        ));

        Ok(Self { task_running })
    }

    //PRIVATE:
    async fn task<F>(
        task_running: Arc<AtomicBool>,
        key: OwnedKeyExpr,
        beacon_period: Duration,
        session: Arc<Session>,
        on_resource_declared: F,
        on_resource_undeclared: F,
    ) -> ZResult<()>
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        let accumulating_resources = Mutex::new(HashMap::<OwnedKeyExpr, AlohaResource>::new());
        let subscriber = session.declare_subscriber(key).await?;

        let listen = Self::listening_task(
            task_running.clone(),
            &accumulating_resources,
            &subscriber,
            &on_resource_declared,
        )
        .fuse();

        let listen_timeout = Self::accumulating_task(
            task_running,
            beacon_period * 3,
            &accumulating_resources,
            on_resource_undeclared,
        )
        .fuse();

        join!(listen, listen_timeout);

        Ok(())
    }

    async fn listening_task<'a, F>(
        task_running: Arc<AtomicBool>,
        accumulating_resources: &Mutex<HashMap<OwnedKeyExpr, AlohaResource>>,
        subscriber: &'a zenoh::pubsub::Subscriber<'a, Receiver<Sample>>,
        on_resource_declared: &F,
    ) where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        while task_running.load(Relaxed) {
            match subscriber.recv_async().await {
                Ok(val) => match accumulating_resources
                    .lock()
                    .await
                    .entry(val.key_expr().as_keyexpr().into())
                {
                    Occupied(mut val) => {
                        val.get_mut().update();
                    }
                    Vacant(entry) => {
                        on_resource_declared(entry.key().into()).await;
                        entry.insert(AlohaResource::new());
                    }
                },
                Err(e) => {
                    error!("Listening error: {}", e);
                }
            }
        }
    }

    async fn accumulating_task<'a, F>(
        task_running: Arc<AtomicBool>,
        accumulate_period: Duration,
        accumulating_resources: &Mutex<HashMap<OwnedKeyExpr, AlohaResource>>,
        on_resource_undeclared: F,
    ) where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        while task_running.load(Relaxed) {
            accumulating_resources
                .lock()
                .await
                .iter_mut()
                .for_each(|val| {
                    val.1.reset();
                });

            tokio::time::sleep(accumulate_period).await;

            for (key, val) in accumulating_resources.lock().await.iter() {
                if !val.is_active() {
                    on_resource_undeclared(key.into()).await;
                }
            }

            accumulating_resources
                .lock()
                .await
                .retain(|_key, val| val.is_active());
        }
    }
}

pub struct AlohaSubscriptionBuilder {
    session: Arc<Session>,
    key: OwnedKeyExpr,
    beacon_period: Duration,

    on_resource_declared: Option<Box<TCallback>>,
    on_resource_undeclared: Option<Box<TCallback>>,
}

impl AlohaSubscriptionBuilder {
    pub fn new(session: Arc<Session>, key: OwnedKeyExpr, beacon_period: Duration) -> Self {
        Self {
            session,
            key,
            beacon_period,
            on_resource_declared: None,
            on_resource_undeclared: None,
        }
    }

    pub fn on_resource_declared<F>(mut self, on_resource_declared: F) -> Self
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        self.on_resource_declared = Some(Box::new(on_resource_declared));
        self
    }

    pub fn on_resource_undeclared<F>(mut self, on_resource_undeclared: F) -> Self
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output = ()> + Unpin + Send>
            + Send
            + Sync
            + 'static,
    {
        self.on_resource_undeclared = Some(Box::new(on_resource_undeclared));
        self
    }

    pub async fn build(self) -> ZResult<AlohaSubscription> {
        AlohaSubscription::new(
            self.session,
            self.key,
            self.beacon_period,
            self.on_resource_declared
                .unwrap_or(Box::new(|_dummy| Box::new(Box::pin(async {})))),
            self.on_resource_undeclared
                .unwrap_or(Box::new(|_dummy| Box::new(Box::pin(async {})))),
        )
        .await
    }
}
