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

use std::fmt::Display;

use tracing::debug;
use zenoh::{
    handlers::FifoChannelHandler,
    key_expr::KeyExpr,
    qos::{CongestionControl, Reliability},
    query::Selector,
    sample::{Locality, Sample},
    Result as ZResult, Session,
};

pub struct ZenohClient {
    session: Session,
}

impl ZenohClient {
    // PUBLIC
    pub fn new(session: Session) -> ZenohClient {
        ZenohClient { session }
    }

    pub async fn subscribe<'b, C, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
        callback: C,
    ) -> ZResult<zenoh::pubsub::Subscriber<()>>
    where
        C: Fn(Sample) + Send + Sync + 'static,
        TryIntoKeyExpr: TryInto<KeyExpr<'b>> + Display,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    {
        debug!("Creating Subscriber on {}", key_expr);

        self.session
            .declare_subscriber(key_expr)
            .callback(callback)
            .undeclare_on_drop(true)
            .allowed_origin(Locality::Remote)
            .await
    }

    pub async fn publish<'b: 'static, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> ZResult<zenoh::pubsub::Publisher<'static>>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>> + Display,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    {
        debug!("Creating Publisher on {}", key_expr);

        self.session
            .declare_publisher(key_expr)
            .reliability(Reliability::Reliable)
            .allowed_destination(Locality::Remote)
            .congestion_control(CongestionControl::Block)
            .await
    }

    pub async fn make_queryable<'b, Callback, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
        callback: Callback,
    ) -> ZResult<zenoh::query::Queryable<()>>
    where
        Callback: Fn(zenoh::query::Query) + Send + Sync + 'static,
        TryIntoKeyExpr: TryInto<KeyExpr<'b>> + Display,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh::Error>,
    {
        debug!("Creating Queryable on {}", key_expr);

        self.session
            .declare_queryable(key_expr)
            .allowed_origin(Locality::Remote)
            .callback(callback)
            .undeclare_on_drop(true)
            .await
    }

    #[cfg(feature = "test")]
    pub async fn make_query<'b, Callback, IntoSelector>(
        &self,
        selector: IntoSelector,
        callback: Callback,
        data: Vec<u8>,
    ) -> ZResult<()>
    where
        Callback: Fn(zenoh::query::Reply) + Send + Sync + 'static,
        IntoSelector: TryInto<Selector<'b>> + Display,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh::Error>,
    {
        debug!("Creating Query on {}", selector);

        self.session
            .get(selector)
            .payload(data)
            .callback(callback)
            .allowed_destination(Locality::Remote)
            .await
    }

    pub async fn make_query_sync<'b, IntoSelector>(
        &self,
        selector: IntoSelector,
        data: Vec<u8>,
    ) -> ZResult<FifoChannelHandler<zenoh::query::Reply>>
    where
        IntoSelector: TryInto<Selector<'b>> + Display,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh::Error>,
    {
        debug!("Creating Query on {}", selector);

        self.session
            .get(selector)
            .payload(data)
            .allowed_destination(Locality::Remote)
            .await
    }
}
