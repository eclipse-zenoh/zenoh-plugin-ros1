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

use rosrust::{Publisher, RawMessage, RawMessageDescription};
use std::{
    collections::{hash_map::Entry, HashMap},
    net::{TcpStream, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::Duration,
};
use zenoh::{
    core::Result as ZResult,
    internal::{bail, zerror},
};

use super::{ros1_client::Ros1Client, topic_descriptor::TopicDescriptor};
use rosrust::RosMsg;

pub type TopicName = String;
pub type NodeName = String;
pub type DataType = String;
pub type Md5 = String;

/**
 * Ros1ResourceCache - caching resolver for ROS1 resources
 */
pub struct Ros1ResourceCache {
    bridge_ros_node_name: String,
    aux_node: Ros1Client,
    service_cache: HashMap<TopicName, HashMap<NodeName, (DataType, Md5)>>,
    publisher_cache: HashMap<TopicName, HashMap<NodeName, (DataType, Md5)>>,
    subscriber_cache: HashMap<TopicName, TopicSubscribersDiscovery>,
}

impl Ros1ResourceCache {
    pub fn new(
        aux_node_name: &str,
        bridge_ros_node_name: String,
        ros_master_uri: &str,
    ) -> ZResult<Ros1ResourceCache> {
        let aux_node = Ros1Client::new(aux_node_name, ros_master_uri)?;
        Ok(Ros1ResourceCache {
            bridge_ros_node_name,
            aux_node,
            service_cache: HashMap::default(),
            publisher_cache: HashMap::default(),
            subscriber_cache: HashMap::default(),
        })
    }

    pub fn resolve_subscriber_parameters(
        &mut self,
        topic: TopicName,
        node: NodeName,
    ) -> ZResult<(DataType, Md5)> {
        if node == self.bridge_ros_node_name {
            bail!("Ignoring our own aux node's resources!");
        }

        match self.subscriber_cache.entry(topic.clone()) {
            Entry::Occupied(occupied) => occupied.get().query(&node),
            Entry::Vacant(vacant) => {
                let resolver = TopicSubscribersDiscovery::new(&self.aux_node, &topic)?;
                vacant.insert(resolver).query(&node)
            }
        }
    }

    pub fn resolve_publisher_parameters(
        &mut self,
        topic: TopicName,
        node: NodeName,
    ) -> ZResult<(DataType, Md5)> {
        if node == self.aux_node.ros.name() || node == self.bridge_ros_node_name {
            bail!("Ignoring our own aux node's resources!");
        }

        match self.publisher_cache.entry(topic.clone()) {
            Entry::Occupied(mut occupied) => {
                match occupied.get_mut().entry(node.clone()) {
                    Entry::Occupied(occupied) => {
                        // return already resolved data
                        Ok(occupied.get().clone())
                    }
                    Entry::Vacant(vacant) => {
                        // actively resolve data through network
                        let data = Self::resolve_publisher(&self.aux_node, topic, &node)?;

                        // save in cache and return data
                        Ok(vacant.insert(data).clone())
                    }
                }
            }
            Entry::Vacant(vacant) => {
                // actively resolve datatype through network
                let data = Self::resolve_publisher(&self.aux_node, topic, &node)?;

                // save in cache and return datatype
                let mut node_map = HashMap::new();
                node_map.insert(node, data.clone());
                vacant.insert(node_map);
                Ok(data)
            }
        }
    }

    pub fn resolve_service_parameters(
        &mut self,
        service: TopicName,
        node: NodeName,
    ) -> ZResult<(DataType, Md5)> {
        if node == self.bridge_ros_node_name {
            bail!("Ignoring our own aux node's resources!");
        }

        match self.service_cache.entry(service.clone()) {
            Entry::Occupied(mut occupied) => {
                match occupied.get_mut().entry(node) {
                    Entry::Occupied(occupied) => {
                        // return already resolved data
                        Ok(occupied.get().clone())
                    }
                    Entry::Vacant(vacant) => {
                        // actively resolve datatype through network
                        let data = Self::resolve_service(&self.aux_node, service)?;

                        // save in cache and return datatype
                        Ok(vacant.insert(data).clone())
                    }
                }
            }
            Entry::Vacant(vacant) => {
                // actively resolve datatype through network
                let data = Self::resolve_service(&self.aux_node, service)?;

                // save in cache and return datatype
                let mut node_map = HashMap::new();
                node_map.insert(node, data.clone());
                vacant.insert(node_map);
                Ok(data)
            }
        }
    }

    fn resolve_service(aux_node: &Ros1Client, service: TopicName) -> ZResult<(DataType, Md5)> {
        let resolving_client = aux_node
            .client(&TopicDescriptor {
                name: service,
                datatype: String::from("*"),
                md5: String::from("*"),
            })
            .map_err(|e| zerror!("{e}"))?;
        let mut probe = resolving_client
            .probe(Duration::from_secs(1))
            .map_err(|e| zerror!("{e}"))?;
        let datatype = probe
            .remove("type")
            .ok_or("no type field in service's responce!")?;
        let md5 = probe
            .remove("md5sum")
            .ok_or("no md5sum field in service's responce!")?;
        Ok((datatype, md5))
    }

    fn resolve_publisher(
        aux_node: &Ros1Client,
        topic: TopicName,
        node: &NodeName,
    ) -> ZResult<(DataType, Md5)> {
        let publisher_uri = aux_node.ros.master.lookup_node(node)?;
        let (protocol, hostname, port) = Self::request_topic(&publisher_uri, "probe", &topic)?;
        if protocol != "TCPROS" {
            bail!("Publisher responded with a non-TCPROS protocol: {protocol}");
        }

        if let Some(address) = (hostname.as_str(), port as u16).to_socket_addrs()?.next() {
            let mut stream = TcpStream::connect(address)?;
            let mut headers = Self::exchange_headers::<_>(&mut stream, topic.clone())?;

            let datatype = headers
                .remove("type")
                .ok_or("no type field in publisher's responce!")?;
            let md5 = headers
                .remove("md5sum")
                .ok_or("no md5sum field in publisher's responce!")?;

            return Ok((datatype, md5));
        }

        bail!("No endpoints found for topic {topic} node name {node}")
    }

    fn request_topic(
        publisher_uri: &str,
        caller_id: &str,
        topic: &str,
    ) -> ZResult<(String, String, i32)> {
        let (_code, _message, protocols): (i32, String, (String, String, i32)) =
            xml_rpc::Client::new()
                .map_err(|e| zerror!("Foreign XmlRpc: {e}"))?
                .call(
                    &publisher_uri
                        .parse()
                        .map_err(|e| zerror!("Bad uri: {publisher_uri} error: {e}"))?,
                    "requestTopic",
                    (caller_id, topic, [["TCPROS"]]),
                )
                .map_err(|e| zerror!("Error making XmlRpc request: {e}"))?
                .map_err(|e| zerror!("Error making XmlRpc request: {}", e.message))?;
        Ok(protocols)
    }

    fn write_request<U: std::io::Write>(mut stream: &mut U, topic: TopicName) -> ZResult<()> {
        let mut fields = HashMap::<String, String>::new();
        fields.insert(String::from("message_definition"), String::from("*"));
        fields.insert(String::from("callerid"), String::from("probe"));
        fields.insert(String::from("topic"), topic);
        fields.insert(String::from("md5sum"), String::from("*"));
        fields.insert(String::from("type"), String::from("*"));
        Self::encode(&mut stream, &fields)?;
        Ok(())
    }

    fn read_response<U: std::io::Read>(mut stream: &mut U) -> ZResult<HashMap<String, String>> {
        let fields = Self::decode(&mut stream)?;
        Ok(fields)
    }

    fn exchange_headers<U>(stream: &mut U, topic: TopicName) -> ZResult<HashMap<String, String>>
    where
        U: std::io::Write + std::io::Read,
    {
        Self::write_request::<U>(stream, topic)?;
        Self::read_response::<U>(stream)
    }

    fn decode<R: std::io::Read>(data: &mut R) -> ZResult<HashMap<String, String>> {
        rosrust::RosMsg::decode(data).map_err(|e| e.into())
    }

    fn encode<W: std::io::Write>(writer: &mut W, data: &HashMap<String, String>) -> ZResult<()> {
        data.encode(writer).map_err(|e| e.into())
    }
}

struct TopicSubscribersDiscovery {
    _discovering_publisher: Publisher<RawMessage>,
    discovered_subscribers: Arc<Mutex<HashMap<NodeName, (DataType, Md5)>>>,
}

impl TopicSubscribersDiscovery {
    fn new(aux_node: &Ros1Client, topic: &TopicName) -> ZResult<Self> {
        let discovered_subscribers = Arc::new(Mutex::new(HashMap::default()));

        let c_discovered_subscribers = discovered_subscribers.clone();
        let on_handshake = move |fields: &HashMap<String, String>| -> bool {
            let _ = Self::process_handshake(fields, c_discovered_subscribers.clone());
            false
        };

        let description = RawMessageDescription {
            msg_definition: "*".to_string(),
            md5sum: "*".to_string(),
            msg_type: "*".to_string(),
        };

        let discovering_publisher = aux_node
            .ros
            .publish_common(topic, 0, Some(description), Some(Box::new(on_handshake)))
            .map_err(|e| e.to_string())?;

        Ok(Self {
            _discovering_publisher: discovering_publisher,
            discovered_subscribers,
        })
    }

    fn query(&self, node: &NodeName) -> ZResult<(DataType, Md5)> {
        let guard = self
            .discovered_subscribers
            .lock()
            .map_err(|e| zerror!("{e}"))?;
        guard
            .get(node)
            .ok_or_else(|| zerror!("No such node: {}", node).into())
            .map(|v| v.clone())
    }

    fn process_handshake(
        fields: &HashMap<String, String>,
        discovered_subscribers: Arc<Mutex<HashMap<NodeName, (DataType, Md5)>>>,
    ) -> ZResult<()> {
        let datatype = fields
            .get("type")
            .ok_or("no type field in subscriber's responce!")?;
        let md5 = fields
            .get("md5sum")
            .ok_or("no md5sum field in subscriber's responce!")?;
        let node_name = fields
            .get("callerid")
            .ok_or("no callerid field in subscriber's responce!")?;

        if let Ok(mut guard) = discovered_subscribers.lock() {
            match guard.entry(node_name.to_owned()) {
                Entry::Occupied(mut occupied) => {
                    occupied.insert((datatype.to_owned(), md5.to_owned()));
                }
                Entry::Vacant(vacant) => {
                    vacant.insert((datatype.to_owned(), md5.to_owned()));
                }
            }
        }

        Ok(())
    }
}
