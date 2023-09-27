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

use super::{ros1_client::Ros1Client, test_helpers::wait_sync};

pub fn wait_for_rosclient_to_connect(rosclient: &Ros1Client) -> bool {
    wait_sync(|| rosclient.state().is_ok(), Duration::from_secs(10))
}

pub fn wait_for_publishers(subscriber: &Subscriber, count: usize) -> bool {
    wait_sync(
        || subscriber.publisher_count() == count,
        Duration::from_secs(10),
    )
}

pub fn wait_for_subscribers(publisher: &Publisher<RawMessage>, count: usize) -> bool {
    wait_sync(
        || publisher.subscriber_count() == count,
        Duration::from_secs(10),
    )
}
