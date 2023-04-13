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

use std::sync::Arc;

use log::{debug, info};

use zenoh::Session;
use zenoh::prelude::r#async::*;
pub use zenoh_core::zresult::ZResult;

pub struct ZenohClient {
    session: Arc<Session>
}

impl ZenohClient {
// PUBLIC
    pub fn new(session: Arc<Session>) -> ZenohClient {
        ZenohClient {
            session
        }
    }

    pub async fn subscribe<C>(&self, key_expr: &str, callback: C) -> ZResult<zenoh::subscriber::Subscriber<'static, ()>>
    where
        C: Fn(Sample) + Send + Sync + 'static
    {
        debug!("Creating Subscriber on {}", key_expr);
    
        match self.make_keyexpr(key_expr).await {
            Ok(opt_keyexpr) => { 
                return self.session.declare_subscriber(opt_keyexpr)
                                   .callback(callback)
                                   .allowed_origin(Locality::Remote).res_async().await;
            }
            Err(e) => { return Err(e); }
        }
    }

    pub async fn publish(&self, key_expr: &str) -> ZResult<zenoh::publication::Publisher<'static>> {
        debug!("Creating Publisher on {}", key_expr);

        match self.make_keyexpr(key_expr).await {
            Ok(opt_keyexpr) => { 
                return self.session.declare_publisher(opt_keyexpr)
                                   .allowed_destination(Locality::Remote)
                                   .congestion_control(CongestionControl::Block).res_async().await;
            }
            Err(e) => { return Err(e); }
        }
    }

    pub async fn make_queryable<Callback>(&self, key_expr: &str, callback: Callback) -> ZResult<zenoh::queryable::Queryable<'static, ()> >
    where
        Callback: Fn(zenoh::queryable::Query) + Send + Sync + 'static,
    {
        info!("Creating Queryable on {}", key_expr);

        match self.make_keyexpr(key_expr).await {
            Ok(opt_keyexpr) => { 
                return self.session.declare_queryable(opt_keyexpr)
                                   .allowed_origin(Locality::Remote)
                                   .callback(callback).res_async().await;
            }
            Err(e) => { return Err(e); }
        }
    }

    pub async fn make_query<Callback>(
        &self, 
        key_expr: &str,
        callback: Callback,
        data: Vec<u8>) -> ZResult<()>
    where
        Callback: Fn(zenoh::query::Reply) + Send + Sync + 'static,
    {
        debug!("Creating Query on {}", key_expr);

        match self.make_keyexpr(key_expr).await {
            Ok(opt_keyexpr) => { 
                return self.session.get(opt_keyexpr)
                                   .with_value(data)
                                   .callback(callback).res_async().await;
            }
            Err(e) => { return Err(e); }
        }
    }

    pub async fn make_query_sync(
        &self, 
        key_expr: &str,
        data: Vec<u8>) -> ZResult<flume::Receiver<zenoh::query::Reply>>
    {
        debug!("Creating Query on {}", key_expr);

        match self.make_keyexpr(key_expr).await {
            Ok(opt_keyexpr) => { 
                return self.session.get(opt_keyexpr)
                                   .with_value(data)
                                   .allowed_destination(Locality::Remote)
                                   .res_async().await;
            }
            Err(e) => { return Err(e); }
        }
    }

// PRIVATE
    async fn make_keyexpr(&self, key_expr: &str) -> ZResult<zenoh::key_expr::KeyExpr<'static>> {
        debug!("Create optimized keyexpr {}", key_expr);
        
        self.session.declare_keyexpr(key_expr.to_string()).res_async().await
    }
}