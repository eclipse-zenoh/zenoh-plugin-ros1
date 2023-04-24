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

use zenoh::prelude::keyexpr;

pub fn make_zenoh_key(topic: &rosrust::api::Topic) -> &str {
    topic.name.trim_start_matches('/').trim_end_matches('/')
}

pub fn make_topic(datatype: &keyexpr, topic_name: &keyexpr) -> Result<rosrust::api::Topic, String> {
    let mut name = topic_name.to_string();
    name.insert(0, '/');
    Ok(rosrust::api::Topic {
        name,
        datatype: datatype.to_string(),
    })
}
