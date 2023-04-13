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

use std::{sync::{Arc, atomic::{AtomicBool, Ordering::Relaxed}}, time::Duration, collections::{BTreeMap, btree_map::Entry::*}, cell::Cell};

use flume::Receiver;
use futures::{Future, FutureExt, select, pin_mut};
use log::error;
use zenoh::{prelude::r#async::*, plugins::ZResult};


struct AlohaResource {
    activity: AtomicBool
}
impl AlohaResource {
    fn new() -> Self { 
        Self { 
            activity: AtomicBool::new(true)
        } 
    }

    pub fn update(&mut self) {
        self.activity.store(true, Relaxed);
    }

    pub fn reset(&mut self) {
        self.activity.store(false, Relaxed);
    }

    pub fn is_active(&self) -> bool {
        return self.activity.load(Relaxed);
    }
}

pub struct AlohaSubscription {
    task_running: Arc<AtomicBool>
}

impl Drop for AlohaSubscription {
    fn drop(&mut self) {
        self.task_running.store(false, Relaxed);
    }
}
impl AlohaSubscription {
    pub async fn new<F>(session: Arc<Session>,
                        key: OwnedKeyExpr,
                        beacon_period: Duration,
                        on_resource_declared: F,
                        on_resource_undeclared: F) -> ZResult<Self>
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        let task_running = Arc::new(AtomicBool::new(true));

        async_std::task::spawn(AlohaSubscription::task(
            task_running.clone(), 
            key,
            beacon_period,
            session,
            on_resource_declared,
            on_resource_undeclared
        ));
        
        return Ok(Self{task_running});
    }

//PRIVATE:
    async fn task<F>(task_running: Arc<AtomicBool>,
                     key: OwnedKeyExpr,
                     beacon_period: Duration,
                     session: Arc<Session>,
                     on_resource_declared: F,
                     on_resource_undeclared: F) -> ZResult<()>
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        let mut accumulating_resources = Cell::new(BTreeMap::<String, AlohaResource>::new());
        let subscriber = session.declare_subscriber(key).res_async().await?;

        Self::accumulating_task(
            task_running, 
            beacon_period*3, 
            &mut accumulating_resources, 
            &subscriber,
            on_resource_declared,
            on_resource_undeclared).await;

        return Ok(());
    }

    async fn listening_task<'a, F>(task_running: Arc<AtomicBool>,
                                   accumulating_resources: &mut Cell<BTreeMap<String, AlohaResource>>,
                                   subscriber: &'a zenoh::subscriber::Subscriber<'a, Receiver<Sample>> ,
                                   on_resource_declared: &F)
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        while task_running.load(Relaxed) {
            match subscriber.recv_async().await {
                Ok(val) => {
                    match accumulating_resources.get_mut().entry(val.key_expr.to_string()) {
                        Occupied(mut val) => {
                            val.get_mut().update();
                        }
                        Vacant(entry) => {
                            entry.insert(AlohaResource::new());
                            on_resource_declared(val.key_expr).await;
                        }
                    }
                }
                Err(e) => {
                    error!("Listening error: {}", e);
                }
            }
        }
    }

    async fn accumulating_task<'a, F>(task_running: Arc<AtomicBool>,
                                      accumulate_period: Duration,
                                      accumulating_resources: &mut Cell<BTreeMap<String, AlohaResource>>,
                                      subscriber: &'a zenoh::subscriber::Subscriber<'a, Receiver<Sample>> ,
                                      on_resource_declared: F,
                                      on_resource_undeclared: F)
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn futures::Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        while task_running.load(Relaxed) {
            accumulating_resources.get_mut().iter_mut().for_each(|val| {
                val.1.reset();
            });

            {
                let listen = 
                Self::listening_task(
                    task_running.clone(),
                    accumulating_resources,
                    &subscriber,
                    &on_resource_declared).fuse();
                let listen_timeout = async_std::task::sleep(accumulate_period).fuse();
                pin_mut!(listen, listen_timeout);
                select! {
                    () = listen => {

                    },
                    () = listen_timeout => {

                    }
                };
            }

            for (key, val) in accumulating_resources.get_mut().iter() {
                if !val.is_active() {
                    unsafe {on_resource_undeclared(zenoh::key_expr::KeyExpr::from_str_uncheckend(key)).await};
                }
            } 

            accumulating_resources.get_mut().retain(|_key, val| {
                return val.is_active();
            });
        }
    }
}


pub struct AlohaSubscriptionBuilder
{
    session: Arc<Session>,
    key: OwnedKeyExpr,
    beacon_period: Duration,
 
    on_resource_declared: Option<Box<dyn Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static>>,
    on_resource_undeclared: Option<Box<dyn Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static>>
}

impl AlohaSubscriptionBuilder
{
    pub fn new(session: Arc<Session>,
               key: OwnedKeyExpr,
               beacon_period: Duration) -> Self { 
        return Self { session, 
                      key, 
                      beacon_period, 
                      on_resource_declared: None,
                      on_resource_undeclared: None };
    }

    pub fn on_resource_declared<F>(mut self, on_resource_declared: F) -> Self
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        self.on_resource_declared = Some(Box::new(on_resource_declared));
        return self;
    }

    pub fn on_resource_undeclared<F>(mut self, on_resource_undeclared: F) -> Self
    where
        F: Fn(zenoh::key_expr::KeyExpr) -> Box<dyn Future<Output=()> + Unpin + Send + Sync> + Send + Sync + 'static
    {
        self.on_resource_undeclared = Some(Box::new(on_resource_undeclared));
        return self;
    }

    pub async fn build(self) -> ZResult<AlohaSubscription> {
        return AlohaSubscription::new(
            self.session,
            self.key, 
            self.beacon_period, 
            self.on_resource_declared.unwrap_or(Box::new(|_dummy|{ Box::new(Box::pin(async {})) })),
            self.on_resource_undeclared.unwrap_or(Box::new(|_dummy|{ Box::new(Box::pin(async {})) })),
        ).await;
    }
}