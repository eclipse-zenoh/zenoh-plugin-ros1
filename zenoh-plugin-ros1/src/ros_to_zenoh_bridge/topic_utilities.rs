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

use zenoh::prelude::{keyexpr, OwnedKeyExpr};

use super::topic_descriptor::TopicDescriptor;

zenoh::kedefine!(
    pub ros_mapping_format: "${data_type:*}/${md5:*}/${topic:**}",
);

pub fn make_topic_key(topic: &TopicDescriptor) -> &str {
    topic.name.trim_start_matches('/').trim_end_matches('/')
}

pub fn make_zenoh_key(topic: &TopicDescriptor) -> OwnedKeyExpr {
    let mut formatter = ros_mapping_format::formatter();
    zenoh::keformat!(
        formatter,
        data_type = hex::encode(topic.datatype.as_bytes()),
        md5 = topic.md5.clone(),
        topic = make_topic_key(topic)
    )
    .unwrap()
}

pub fn make_topic(datatype: &str, md5: &str, topic_name: &keyexpr) -> TopicDescriptor {
    let mut name = topic_name.to_string();
    name.insert(0, '/');
    TopicDescriptor {
        name,
        datatype: datatype.to_string(),
        md5: md5.to_string(),
    }
}
