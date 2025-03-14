use libp2p::{
    core::{Multiaddr, connection::ConnectionId},
    identify, ping, gossipsub, request_response,
    swarm::{
        NetworkBehaviour, ConnectionHandler, FromSwarm, 
        ToSwarm, THandlerInEvent, THandlerOutEvent, THandler,
        ConnectionDenied, IntoConnectionHandler, Endpoint,
        derive_prelude::*,
    },
    PeerId,
};
use libp2p::request_response::ResponseChannel;
use std::{task::{Context, Poll}, collections::HashMap, sync::Arc, fmt::Debug};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use once_cell::sync::Lazy;
use std::{io, future::Future, pin::Pin};
use tokio::sync::RwLock;
use tracing::{debug, warn, info, error};
use futures::AsyncRead;
use crate::config::P2pConfig;
use mcp_core::{Content, Resource};

mod handler;
pub use handler::P2pHandler;

/// MCP protocol name
pub static MCP_PROTOCOL_NAME: Lazy<String> = Lazy::new(|| "/mcp/1.0.0".to_string());

/// Topic name for MCP content messages
pub static MCP_CONTENT_TOPIC: Lazy<String> = Lazy::new(|| "mcp-content".to_string());

/// Topic name for MCP resource messages
pub static MCP_RESOURCE_TOPIC: Lazy<String> = Lazy::new(|| "mcp-resource".to_string());

/// Message types for the MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpMessage {
    /// Content message (broadcast)
    Content(Content),
    /// Resource message (request/response)
    Resource(Resource),
}

/// Event types for P2pBehaviour
#[derive(Debug)]
pub enum P2pBehaviourEvent {
    /// Ping events
    Ping(ping::Event),
    /// Identify events
    Identify(identify::Event),
    /// Request/response events
    RequestResponse(request_response::Event<Vec<u8>, Vec<u8>>),
    /// Gossipsub events
    Gossipsub(gossipsub::Event),
}

impl From<ping::Event> for P2pBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        P2pBehaviourEvent::Ping(event)
    }
}

impl From<identify::Event> for P2pBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        P2pBehaviourEvent::Identify(event)
    }
}

impl From<request_response::Event<Vec<u8>, Vec<u8>>> for P2pBehaviourEvent {
    fn from(event: request_response::Event<Vec<u8>, Vec<u8>>) -> Self {
        P2pBehaviourEvent::RequestResponse(event)
    }
}

impl From<gossipsub::Event> for P2pBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        P2pBehaviourEvent::Gossipsub(event)
    }
}

/// Network behavior for the P2P MCP communication
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "P2pBehaviourEvent")]
pub struct P2pBehaviour {
    pub ping: ping::Behaviour,
    pub identify: identify::Behaviour,
    pub request_response: request_response::Behaviour<McpCodec>,
    pub gossipsub: gossipsub::Behaviour,
}

impl P2pBehaviour {
    /// Process incoming P2P behaviour events
    pub fn handle_event(&mut self, event: P2pBehaviourEvent) {
        match event {
            P2pBehaviourEvent::Ping(ping_event) => {
                match ping_event {
                    ping::Event { peer, connection, result: Ok(duration) } => {
                        debug!("Ping successful to peer {}, RTT: {:?}", peer, duration);
                    }
                    ping::Event { peer, connection, result: Err(error) } => {
                        warn!("Ping failed to peer {}: {:?}", peer, error);
                    }
                }
            },
            P2pBehaviourEvent::Identify(identify_event) => {
                match identify_event {
                    identify::Event::Received { peer_id, info, connection_id } => {
                        debug!("Identified peer {}: {:?}", peer_id, info);
                    }
                    identify::Event::Sent { peer_id, connection_id } => {
                        debug!("Sent identify info to {}", peer_id);
                    }
                    identify::Event::Error { peer_id, error, connection_id } => {
                        warn!("Identify error with peer {}: {:?}", peer_id, error);
                    }
                    _ => {}
                }
            },
            P2pBehaviourEvent::RequestResponse(req_res_event) => {
                match req_res_event {
                    request_response::Event::Message { peer, message, connection_id } => {
                        match message {
                            request_response::Message::Request { request, channel, request_id } => {
                                debug!("Received request {:?} from peer {}", request_id, peer);
                                // Process request and respond
                                let response = self.process_request(peer, &request);
                                if let Some(response_data) = response {
                                    if let Err(e) = self.request_response.send_response(channel, response_data) {
                                        error!("Failed to send response: {:?}", e);
                                    }
                                }
                            }
                            request_response::Message::Response { request_id, response } => {
                                debug!("Received response for request {:?} from peer {}", request_id, peer);
                                // Process response
                                self.process_response(peer, request_id, response);
                            }
                        }
                    }
                    request_response::Event::OutboundFailure { peer, request_id, error, connection_id } => {
                        warn!("Outbound request to {} failed: {:?}", peer, error);
                    }
                    request_response::Event::InboundFailure { peer, error, connection_id, request_id } => {
                        warn!("Inbound request from {} failed: {:?}", peer, error);
                    }
                    request_response::Event::ResponseSent { peer, connection_id, request_id } => {
                        debug!("Response sent to {} for request {:?}", peer, request_id);
                    }
                }
            },
            P2pBehaviourEvent::Gossipsub(gossipsub_event) => {
                match gossipsub_event {
                    gossipsub::Event::Message {
                        propagation_source,
                        message_id,
                        message,
                    } => {
                        debug!(
                            "Received gossipsub message from {} with id {:?}",
                            propagation_source, message_id
                        );
                        // Process the message
                        if let Ok(mcp_message) = serde_json::from_slice::<McpMessage>(&message.data) {
                            match mcp_message {
                                McpMessage::Content(content) => {
                                    debug!("Received content: {:?}", content);
                                }
                                McpMessage::Resource(resource) => {
                                    debug!("Received resource: {:?}", resource);
                                }
                            }
                        }
                    }
                    gossipsub::Event::Subscribed { peer_id, topic } => {
                        debug!("Peer {} subscribed to topic {}", peer_id, topic);
                    }
                    gossipsub::Event::Unsubscribed { peer_id, topic } => {
                        debug!("Peer {} unsubscribed from topic {}", peer_id, topic);
                    }
                    _ => {}
                }
            }
        }
    }

    /// Process an incoming request
    fn process_request(&self, peer: PeerId, request: &[u8]) -> Option<Vec<u8>> {
        match serde_json::from_slice::<Resource>(request) {
            Ok(resource) => {
                debug!("Processing resource request from {}: {}", peer, resource.uri);
                // Create a mock response for now
                let response = Resource {
                    uri: resource.uri,
                    name: "Response".to_string(),
                    description: Some("Resource response".to_string()),
                    mime_type: "text".to_string(),
                    annotations: None,
                };
                
                match serde_json::to_vec(&response) {
                    Ok(data) => Some(data),
                    Err(e) => {
                        error!("Failed to serialize response: {:?}", e);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to deserialize request from {}: {:?}", peer, e);
                None
            }
        }
    }

    /// Process a response
    fn process_response(&self, peer: PeerId, request_id: request_response::RequestId, response: Vec<u8>) {
        match serde_json::from_slice::<Resource>(&response) {
            Ok(resource) => {
                debug!("Received resource response from {}: {}", peer, resource.uri);
            }
            Err(e) => {
                error!("Failed to deserialize response from {}: {:?}", peer, e);
            }
        }
    }
}

/// Codec for MCP messages
#[derive(Debug, Clone)]
pub struct McpCodec;

/// Implementation of the request_response::Codec trait for McpCodec
impl request_response::Codec for McpCodec {
    type Protocol = String;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    /// Read a request from the wire
    fn read_request<'a, T>(
        &'a mut self,
        _protocol: &'a Self::Protocol,
        io: &'a mut T,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Request, io::Error>> + Send + 'a>>
    where
        T: AsyncRead + Unpin + Send,
    {
        Box::pin(async move {
            let mut length_bytes = [0u8; 8];
            futures::io::AsyncReadExt::read_exact(io, &mut length_bytes).await?;
            let length = u64::from_be_bytes(length_bytes) as usize;
            
            let mut buffer = vec![0u8; length];
            futures::io::AsyncReadExt::read_exact(io, &mut buffer).await?;
            
            Ok(buffer)
        })
    }

    /// Read a response from the wire
    fn read_response<'a, T>(
        &'a mut self,
        _protocol: &'a Self::Protocol,
        io: &'a mut T,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Response, io::Error>> + Send + 'a>>
    where
        T: AsyncRead + Unpin + Send,
    {
        Box::pin(async move {
            let mut length_bytes = [0u8; 8];
            futures::io::AsyncReadExt::read_exact(io, &mut length_bytes).await?;
            let length = u64::from_be_bytes(length_bytes) as usize;
            
            let mut buffer = vec![0u8; length];
            futures::io::AsyncReadExt::read_exact(io, &mut buffer).await?;
            
            Ok(buffer)
        })
    }

    /// Write a request to the wire
    fn write_request<'a, T>(
        &'a mut self,
        _protocol: &'a Self::Protocol,
        io: &'a mut T,
        req: Self::Request,
    ) -> Pin<Box<dyn Future<Output = Result<(), io::Error>> + Send + 'a>>
    where
        T: futures::io::AsyncWrite + Unpin + Send,
    {
        Box::pin(async move {
            let length = req.len() as u64;
            let length_bytes = length.to_be_bytes();
            futures::io::AsyncWriteExt::write_all(io, &length_bytes).await?;
            futures::io::AsyncWriteExt::write_all(io, &req).await?;
            futures::io::AsyncWriteExt::flush(io).await?;
            Ok(())
        })
    }

    /// Write a response to the wire
    fn write_response<'a, T>(
        &'a mut self,
        _protocol: &'a Self::Protocol,
        io: &'a mut T,
        res: Self::Response,
    ) -> Pin<Box<dyn Future<Output = Result<(), io::Error>> + Send + 'a>>
    where
        T: futures::io::AsyncWrite + Unpin + Send,
    {
        Box::pin(async move {
            let length = res.len() as u64;
            let length_bytes = length.to_be_bytes();
            futures::io::AsyncWriteExt::write_all(io, &length_bytes).await?;
            futures::io::AsyncWriteExt::write_all(io, &res).await?;
            futures::io::AsyncWriteExt::flush(io).await?;
            Ok(())
        })
    }
}

impl P2pBehaviour {
    /// Create a new P2P behavior with the given configuration
    pub async fn new(config: P2pConfig, peer_id: PeerId) -> Result<Self, String> {
        let mut behaviour = Self::default();
        
        // Configure ping for keepalive
        if config.protocol.enable_keepalive {
            behaviour.ping = ping::Behaviour::new(ping::Config::new()
                .with_interval(std::time::Duration::from_secs(
                    config.protocol.keepalive_interval
                ))
            );
        }
        
        // Configure gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(std::time::Duration::from_secs(10))
            .max_transmit_size(config.protocol.max_message_size as usize)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .map_err(|e| format!("Failed to build gossipsub config: {}", e))?;
            
        behaviour.gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(peer_id),
            gossipsub_config
        )
        .map_err(|e| format!("Failed to create gossipsub: {}", e))?;
        
        // Subscribe to MCP topics
        let content_topic = gossipsub::IdentTopic::new(MCP_CONTENT_TOPIC.clone());
        let resource_topic = gossipsub::IdentTopic::new(MCP_RESOURCE_TOPIC.clone());
        
        behaviour.gossipsub.subscribe(&content_topic)
            .map_err(|e| format!("Failed to subscribe to content topic: {}", e))?;
            
        behaviour.gossipsub.subscribe(&resource_topic)
            .map_err(|e| format!("Failed to subscribe to resource topic: {}", e))?;
        
        Ok(behaviour)
    }
    
    /// Distribute content to peers
    pub async fn distribute_content(&mut self, content: Content) -> Result<(), String> {
        let content_msg = McpMessage::Content(content);
        let serialized = serde_json::to_vec(&content_msg)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;
            
        let topic = gossipsub::IdentTopic::new(MCP_CONTENT_TOPIC.clone());
        self.gossipsub.publish(topic, serialized)
            .map_err(|e| format!("Failed to publish content: {}", e))?;
            
        Ok(())
    }
    
    /// Distribute resource to peers
    pub async fn distribute_resource(&mut self, resource: Resource) -> Result<(), String> {
        let resource_msg = McpMessage::Resource(resource);
        let serialized = serde_json::to_vec(&resource_msg)
            .map_err(|e| format!("Failed to serialize resource: {}", e))?;
            
        let topic = gossipsub::IdentTopic::new(MCP_RESOURCE_TOPIC.clone());
        self.gossipsub.publish(topic, serialized)
            .map_err(|e| format!("Failed to publish resource: {}", e))?;
            
        Ok(())
    }
    
    /// Send a request for a specific resource
    pub async fn request_resource(&mut self, peer: PeerId, resource_id: String) -> Result<(), String> {
        // Create a resource request
        let resource = Resource {
            uri: resource_id,
            name: "Request".to_string(),
            description: Some("Resource request".to_string()),
            mime_type: "text".to_string(),
            annotations: None,
        };
        
        // Serialize the request
        let request_data = serde_json::to_vec(&resource)
            .map_err(|e| format!("Failed to serialize resource request: {}", e))?;
            
        // Send the request
        self.request_response.send_request(&peer, request_data);
        
        Ok(())
    }
}

impl Default for P2pBehaviour {
    fn default() -> Self {
        let ping = ping::Behaviour::new(ping::Config::new());
        let identify = identify::Behaviour::new(identify::Config::new(
            "/mcp/1.0.0".to_string(),
            libp2p::identity::Keypair::generate_ed25519().public(),
        ));
        
        // Create default gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(std::time::Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .expect("Valid gossipsub config");
            
        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(
                libp2p::identity::Keypair::generate_ed25519(),
            ),
            gossipsub_config,
        ).expect("Valid gossipsub behavior");
        
        // Create request/response protocol
        let request_response = request_response::Behaviour::with_codec(
            McpCodec {},
            [(MCP_PROTOCOL_NAME.clone(), request_response::ProtocolSupport::Full)],
            request_response::Config::default(),
        );
        
        Self {
            ping,
            identify,
            request_response,
            gossipsub,
        }
    }
}
