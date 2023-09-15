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

use rosrust::api::Topic;
use std::{
    collections::{hash_map::Entry, HashMap},
    time::Duration,
};
use zenoh_core::{zerror, zresult::ZResult};

use super::ros1_client::Ros1Client;

type ServiceName = String;
type NodeName = String;
type DataType = String;

/**
 * Ros1ServiceCache - caching resolver for service's data type
 */
pub struct Ros1ServiceCache {
    aux_node: Ros1Client,
    cache: HashMap<ServiceName, HashMap<NodeName, DataType>>,
}

impl Ros1ServiceCache {
    pub fn new(aux_ros_node_name: &str, ros_master_uri: &str) -> ZResult<Ros1ServiceCache> {
        let aux_node = Ros1Client::new(aux_ros_node_name, ros_master_uri)?;
        Ok(Ros1ServiceCache {
            aux_node,
            cache: HashMap::default(),
        })
    }

    pub fn resolve_datatype(&mut self, service: ServiceName, node: NodeName) -> ZResult<DataType> {
        match self.cache.entry(service.clone()) {
            Entry::Occupied(mut occupied) => {
                match occupied.get_mut().entry(node) {
                    Entry::Occupied(occupied) => {
                        // return already resolved datatype
                        Ok(occupied.get().clone())
                    }
                    Entry::Vacant(vacant) => {
                        // actively resolve datatype through network
                        let datatype = Self::resolve(&self.aux_node, service)?;

                        // save in cache and return datatype
                        Ok(vacant.insert(datatype).clone())
                    }
                }
            }
            Entry::Vacant(vacant) => {
                // actively resolve datatype through network
                let datatype = Self::resolve(&self.aux_node, service)?;

                // save in cache and return datatype
                let mut node_map = HashMap::new();
                node_map.insert(node, datatype.clone());
                vacant.insert(node_map);
                Ok(datatype)
            }
        }
    }

    fn resolve(aux_node: &Ros1Client, service: ServiceName) -> ZResult<DataType> {
        let resolving_client = aux_node
            .client(&Topic {
                name: service,
                datatype: "*".to_string(),
            })
            .map_err(|e| zerror!("{e}"))?;
        let mut probe = resolving_client
            .probe(Duration::from_secs(1))
            .map_err(|e| zerror!("{e}"))?;
        let datatype = probe
            .remove("type")
            .ok_or("no data_type field in service's responce!")?;
        Ok(datatype)
    }
}
