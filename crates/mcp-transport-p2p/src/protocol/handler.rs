use crate::protocol::McpMessage;
use libp2p::{PeerId, request_response::ResponseChannel, request_response::OutboundRequestId, request_response::OutboundFailure};
use mcp_core::{Content, Resource, ResourceContents};
use std::{collections::{HashMap, HashSet}, sync::Arc, time::{Duration, Instant}};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Protocol name for MCP content sharing
#[derive(Debug, Clone)]
pub struct ProtocolName;

impl AsRef<str> for ProtocolName {
    fn as_ref(&self) -> &str {
        "/mcp/content/1.0.0"
    }
}

/// Maximum time (in seconds) we should keep a connection if no messages are sent/received
pub const MAX_IDLE_CONNECTION_SECS: u64 = 900; // 15 minutes

/// Maximum size of MCP messages (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1_048_576;

/// Transport protocol type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportType {
    /// WebTransport protocol
    WebTransport,
    /// WebRTC protocol
    WebRTC,
    /// Unknown or unspecified protocol
    Unknown,
}

/// Message types for P2P communication (internal format)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ProtocolMessage {
    /// Content message (tools, text, etc.)
    Content(Content),
    /// Resource message (images, files, etc.)
    Resource(Resource),
    /// Request for a specific resource by ID
    ResourceRequest(String),
    /// Response to a resource request
    ResourceResponse {
        id: String,
        contents: Option<ResourceContents>,
    },
    /// Ping message to check connectivity
    Ping(Uuid),
    /// Pong response to a ping
    Pong(Uuid),
}

/// P2P Handler for MCP protocol messages
pub struct P2pHandler {
    /// Connected peers and their state
    pub peer_states: Arc<RwLock<HashMap<PeerId, PeerState>>>,
    /// Pending outbound requests
    pub pending_requests: Arc<RwLock<HashMap<OutboundRequestId, PendingRequest>>>,
    /// Resource cache (URI -> Resource)
    pub resource_cache: Arc<RwLock<HashMap<String, Resource>>>,
    /// Gossipsub topic (if enabled)
    pub gossipsub_topic: Option<String>,
    /// Current transport type
    transport_type: TransportType,
    /// Seen message IDs for deduplication (only used with gossipsub)
    seen_messages: Arc<RwLock<HashMap<String, Instant>>>,
}

/// State for a connected peer
#[derive(Debug, Clone)]
pub struct PeerState {
    /// Peer ID
    pub peer_id: PeerId,
    /// Last time this peer was seen
    pub last_seen: Instant,
    /// Number of messages sent to this peer
    pub messages_sent: usize,
    /// Number of messages received from this peer
    pub messages_received: usize,
    /// Transport protocol used by this peer
    pub transport_type: TransportType,
}

/// Pending request information
#[derive(Debug, Clone)]
pub struct PendingRequest {
    /// Time the request was sent
    pub time: Instant,
    /// Request type
    pub req_type: RequestType,
}

/// Request type
#[derive(Debug, Clone)]
pub enum RequestType {
    /// Generic request
    Generic,
    /// Content request
    Content(String),
    /// Resource request
    Resource(String),
}

impl P2pHandler {
    /// Create a new P2P handler
    pub fn new() -> Self {
        Self {
            peer_states: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            gossipsub_topic: None,
            resource_cache: Arc::new(RwLock::new(HashMap::new())),
            transport_type: TransportType::Unknown,
            seen_messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a P2P handler with a specific transport type
    pub fn with_transport(transport_type: TransportType) -> Self {
        Self {
            peer_states: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            gossipsub_topic: None,
            resource_cache: Arc::new(RwLock::new(HashMap::new())),
            transport_type,
            seen_messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the current transport type
    pub fn set_transport_type(&mut self, transport_type: TransportType) {
        self.transport_type = transport_type;
    }

    /// Get the current default transport type
    pub fn get_transport_type(&self) -> &TransportType {
        &self.transport_type
    }

    /// Register a content resource
    pub async fn register_content(&self, content: Content) -> Result<(), String> {
        // Create a resource from the content
        let resource = Resource {
            uri: format!("p2p://{}", Uuid::new_v4().to_string()),
            name: format!("P2P Content {}", Uuid::new_v4().to_string()),
            description: Some("Content shared via P2P transport".to_string()),
            mime_type: "application/octet-stream".to_string(),
            annotations: None,
        };

        // Store in cache
        let mut cache = self.resource_cache.write().await;
        cache.insert(resource.uri.clone(), resource);

        Ok(())
    }

    /// Lookup a resource by URI
    pub async fn lookup_resource(&self, uri: &str) -> Option<Resource> {
        let cache = self.resource_cache.read().await;
        cache.get(uri).cloned()
    }

    /// Process an incoming message and determine how to respond
    pub async fn process_message(&self, peer: PeerId, msg: McpMessage) -> Option<McpMessage> {
        // Update peer state
        self.update_peer_state(peer, false).await;
        
        match msg {
            McpMessage::Content(content) => {
                debug!("Received content from peer {}", peer);
                
                // Store the content
                if let Err(e) = self.register_content(content.clone()).await {
                    error!("Failed to register content: {}", e);
                }
                
                None
            }
            McpMessage::Resource(resource) => {
                debug!("Received resource from peer {}: {}", peer, resource.uri);
                
                // Store the resource
                let mut cache = self.resource_cache.write().await;
                cache.insert(resource.uri.clone(), resource.clone());
                
                None
            }
        }
    }

    /// Handle incoming protocol message (internal format)
    pub async fn process_protocol_message(&self, peer: PeerId, msg: ProtocolMessage) -> Option<ProtocolMessage> {
        // Update peer state
        self.update_peer_state(peer, false).await;

        match msg {
            ProtocolMessage::Content(content) => {
                debug!("Received content message from {}", peer);
                // Store the content
                if let Err(e) = self.register_content(content.clone()).await {
                    error!("Failed to register content: {}", e);
                }
                None
            }
            ProtocolMessage::Resource(resource) => {
                debug!("Received resource message from {}", peer);
                // Store the resource
                let mut cache = self.resource_cache.write().await;
                cache.insert(resource.uri.clone(), resource);
                None
            }
            ProtocolMessage::ResourceRequest(uri) => {
                debug!("Received resource request from {} for {}", peer, uri);
                // Respond with the requested resource
                let resource_cache = self.resource_cache.read().await;
                
                // In libp2p 0.55.0, we need to handle ResourceContents differently
                // Create a dummy response if the resource isn't found
                let contents_response = match resource_cache.get(&uri) {
                    Some(resource) => Some(ResourceContents::TextResourceContents {
                        uri: resource.uri.clone(),
                        mime_type: Some(resource.mime_type.clone()),
                        text: "Resource content placeholder".to_string(),
                    }),
                    None => None
                };
                
                Some(ProtocolMessage::ResourceResponse {
                    id: uri,
                    contents: contents_response,
                })
            }
            ProtocolMessage::ResourceResponse { id, contents } => {
                debug!("Received resource response from {} for {}", peer, id);
                if let Some(contents) = contents {
                    // Create a resource from the response
                    let resource = Resource {
                        uri: id.clone(),
                        name: format!("Resource-{}", id),
                        description: Some("P2P received resource".to_string()),
                        mime_type: "application/octet-stream".to_string(),
                        annotations: None,
                    };
                    
                    // Store in cache
                    let mut cache = self.resource_cache.write().await;
                    cache.insert(id, resource);
                } else {
                    warn!("Received resource {} with no contents", id);
                }
                None
            }
            ProtocolMessage::Ping(id) => {
                debug!("Received ping from {}: {}", peer, id);
                Some(ProtocolMessage::Pong(id))
            }
            ProtocolMessage::Pong(id) => {
                debug!("Received pong from {}: {}", peer, id);
                None
            }
        }
    }

    /// Handle request-response protocol message
    pub async fn handle_request(&self, peer: PeerId, request: Vec<u8>, channel: ResponseChannel<Vec<u8>>) {
        // Update peer state
        self.update_peer_state(peer, false).await;
        
        // Process request using the process_request method
        match self.process_request(peer, &request).await {
            Some(response) => {
                if let Err(e) = channel.respond(response) {
                    error!("Failed to send response to {}: {}", peer, e);
                }
            }
            None => {
                // Send empty response
                if let Err(e) = channel.respond(Vec::new()) {
                    error!("Failed to send empty response: {}", e);
                }
            }
        }
    }

    /// Process an incoming request and generate a response
    async fn process_request(&self, peer: PeerId, request: &[u8]) -> Option<Vec<u8>> {
        // Try to parse as a ProtocolMessage first
        if let Ok(message) = serde_json::from_slice::<ProtocolMessage>(request) {
            match message {
                ProtocolMessage::ResourceRequest(uri) => {
                    debug!("Received resource request for {}", uri);
                    
                    // Look up the resource
                    let resource = self.lookup_resource(&uri).await;
                    
                    let response = match resource {
                        Some(res) => ProtocolMessage::Resource(res),
                        None => {
                            warn!("Resource not found: {}", uri);
                            ProtocolMessage::ResourceResponse {
                                id: uri,
                                contents: None,
                            }
                        }
                    };
                    
                    // Return the serialized response
                    match serde_json::to_vec(&response) {
                        Ok(encoded) => {
                            debug!("Sending resource response");
                            return Some(encoded);
                        }
                        Err(e) => {
                            error!("Failed to encode response: {:?}", e);
                            return None;
                        }
                    }
                }
                ProtocolMessage::Content(content) => {
                    debug!("Received content request");
                    
                    // Process the content
                    if let Err(e) = self.register_content(content.clone()).await {
                        error!("Failed to register content: {}", e);
                    }
                    
                    // Send acknowledgement
                    let response = ProtocolMessage::Pong(Uuid::new_v4());
                    match serde_json::to_vec(&response) {
                        Ok(encoded) => {
                            debug!("Sending content acknowledgement");
                            return Some(encoded);
                        }
                        Err(e) => {
                            error!("Failed to encode response: {:?}", e);
                            return None;
                        }
                    }
                }
                _ => {
                    debug!("Received unhandled protocol message type");
                    return None;
                }
            }
        }
        
        // Try to parse as a Resource as fallback (original implementation)
        match serde_json::from_slice::<Resource>(request) {
            Ok(resource) => {
                debug!("Processing resource request from {}: {}", peer, resource.uri);
                // Create a response
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

    /// Broadcast a resource to all known peers
    pub async fn broadcast_resource(&self, resource: Resource) -> Result<(), String> {
        let resource_uri = resource.uri.clone();
        
        // Cache the resource
        let mut resources = self.resource_cache.write().await;
        resources.insert(resource_uri.clone(), resource.clone());
        
        info!("Broadcasting resource: {}", resource_uri);
        
        // Broadcast to all connected peers
        let peers = self.peer_states.read().await;
        for peer_id in peers.keys() {
            debug!("Broadcasting resource {} to peer {}", resource_uri, peer_id);
            
            // This is a stub - the actual broadcasting will be done in the Behaviour
            // which will handle both gossipsub and request-response protocols
        }
        
        Ok(())
    }

    /// Get a resource by URI
    pub async fn get_resource(&self, resource_uri: &str) -> Option<Resource> {
        let resources = self.resource_cache.read().await;
        resources.get(resource_uri).cloned()
    }

    /// Update the state for a peer
    pub async fn update_peer_state(&self, peer: PeerId, is_outgoing: bool) {
        let mut peers = self.peer_states.write().await;
        let peer_state = peers.entry(peer).or_insert_with(|| PeerState {
            peer_id: peer,
            last_seen: std::time::Instant::now(),
            messages_sent: 0,
            messages_received: 0,
            transport_type: self.transport_type.clone(),
        });
        
        peer_state.last_seen = std::time::Instant::now();
        if is_outgoing {
            peer_state.messages_sent += 1;
        } else {
            peer_state.messages_received += 1;
        }
    }

    /// Clean up peers that haven't been seen recently
    pub async fn cleanup_peers(&self, timeout: Duration) {
        let mut peers = self.peer_states.write().await;
        let now = std::time::Instant::now();
        peers.retain(|_, state| now.duration_since(state.last_seen) < timeout);
    }

    /// Register a new peer with transport type
    pub async fn register_peer(&self, peer: PeerId, transport_type: TransportType) {
        let mut states = self.peer_states.write().await;
        
        debug!("Registering peer {} with transport {:?}", peer, transport_type);
        states.insert(peer, PeerState {
            peer_id: peer,
            last_seen: Instant::now(),
            messages_sent: 0,
            messages_received: 0,
            transport_type,
        });
    }

    /// Unregister a peer (e.g., when disconnected)
    pub async fn unregister_peer(&self, peer: PeerId) {
        let mut states = self.peer_states.write().await;
        if states.remove(&peer).is_some() {
            debug!("Unregistered peer {}", peer);
        }
    }

    /// Handle gossipsub message
    pub async fn handle_gossipsub(&self, peer: PeerId, msg_id: String, data: Vec<u8>) -> bool {
        // Check if we've already seen this message
        let mut seen = self.seen_messages.write().await;
        if seen.contains_key(&msg_id) {
            debug!("Ignoring duplicate gossipsub message from {}", peer);
            return false;
        }
        
        // Mark as seen
        seen.insert(msg_id, Instant::now());
        
        // Update peer state
        self.update_peer_state(peer, false).await;
        
        // Try to deserialize as McpMessage first (standard format)
        match serde_json::from_slice::<McpMessage>(&data) {
            Ok(message) => {
                debug!("Processing gossipsub message from peer {} as McpMessage", peer);
                self.process_message(peer, message).await;
                true
            }
            Err(_) => {
                // Try as ProtocolMessage (internal format)
                match serde_json::from_slice::<ProtocolMessage>(&data) {
                    Ok(message) => {
                        debug!("Processing gossipsub message from peer {} as ProtocolMessage", peer);
                        self.process_protocol_message(peer, message).await;
                        true
                    }
                    Err(e) => {
                        error!("Failed to parse gossipsub message from {}: {}", peer, e);
                        false
                    }
                }
            }
        }
    }

    /// Handle request-response protocol message
    pub async fn handle_response(&self, peer: PeerId, request_id: OutboundRequestId, 
                                 result: Result<Vec<u8>, OutboundFailure>) {
        // Update peer state
        self.update_peer_state(peer, false).await;
        
        // Remove from pending requests
        let req_type = {
            let mut pending = self.pending_requests.write().await;
            pending.remove(&request_id).map(|p| p.req_type)
        };
        
        match result {
            Ok(data) => {
                if data.is_empty() {
                    debug!("Received empty response from {}", peer);
                    return;
                }
                
                // Try to deserialize as McpMessage first (standard format)
                match serde_json::from_slice::<McpMessage>(&data) {
                    Ok(message) => {
                        match message {
                            McpMessage::Resource(resource) => {
                                debug!("Received resource response from peer {}: {}", peer, resource.uri);
                                
                                // Cache the resource
                                let mut resources = self.resource_cache.write().await;
                                resources.insert(resource.uri.clone(), resource);
                            }
                            McpMessage::Content(content) => {
                                debug!("Received content response from peer {}: {}", peer, content.name);
                                // Process content if needed
                                if let Err(e) = self.register_content(content).await {
                                    error!("Failed to register content: {}", e);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Try as ProtocolMessage (internal format)
                        match serde_json::from_slice::<ProtocolMessage>(&data) {
                            Ok(message) => {
                                match (message, req_type) {
                                    (ProtocolMessage::ResourceResponse { id, contents }, Some(RequestType::Resource(req_id))) => {
                                        if id == req_id {
                                            debug!("Received requested resource {} from {}", id, peer);
                                            if let Some(contents) = contents {
                                                // Create a resource from the response
                                                let resource = Resource {
                                                    uri: id.clone(),
                                                    name: format!("Resource-{}", id),
                                                    description: Some("P2P received resource".to_string()),
                                                    mime_type: "application/octet-stream".to_string(),
                                                    annotations: None,
                                                };
                                                
                                                // Store in cache
                                                let mut cache = self.resource_cache.write().await;
                                                cache.insert(id, resource);
                                            } else {
                                                warn!("Received resource {} with no contents", id);
                                            }
                                        } else {
                                            warn!("Received resource {} but requested {}", id, req_id);
                                        }
                                    }
                                    (ProtocolMessage::Resource(resource), _) => {
                                        debug!("Received resource from {}: {}", peer, resource.uri);
                                        // Store the resource
                                        let mut cache = self.resource_cache.write().await;
                                        cache.insert(resource.uri.clone(), resource);
                                    }
                                    (ProtocolMessage::Pong(id), _) => {
                                        debug!("Received pong from {} for {}", peer, id);
                                    }
                                    _ => {
                                        debug!("Received unexpected response type from {}", peer);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse response from {}: {}", peer, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Request to {} failed: {:?}", peer, e);
            }
        }
    }
    
    /// Process a response message (original implementation)
    async fn process_response(&self, peer: PeerId, msg: McpMessage, req_type: Option<RequestType>) {
        match (msg, req_type) {
            (McpMessage::Resource(resource), Some(RequestType::Resource(id))) => {
                debug!("Received requested resource {} from {}", id, peer);
                
                // Store the resource
                let mut cache = self.resource_cache.write().await;
                cache.insert(resource.uri.clone(), resource.clone());
            }
            _ => {
                debug!("Received general response from {}", peer);
            }
        }
    }

    /// Get a list of all known peers
    pub async fn get_known_peers(&self) -> HashSet<PeerId> {
        self.peer_states.read().await.keys().cloned().collect()
    }
    
    /// Get peer count by transport type
    pub async fn get_peer_count_by_transport(&self, transport_type: Option<TransportType>) -> usize {
        let states = self.peer_states.read().await;
        
        match transport_type {
            Some(tp) => states.values()
                .filter(|state| state.transport_type == tp)
                .count(),
            None => states.len(),
        }
    }
    
    /// Clean up stale peers that haven't been active for the specified duration
    pub async fn clean_stale_peers(&self, timeout: Duration) {
        let mut states = self.peer_states.write().await;
        let now = Instant::now();
        
        // Get peers to remove
        let stale_peers: Vec<PeerId> = states
            .iter()
            .filter(|(_, state)| now.duration_since(state.last_seen) > timeout)
            .map(|(peer, _)| *peer)
            .collect();
            
        // Remove stale peers
        for peer in &stale_peers {
            debug!("Removing stale peer: {}", peer);
            states.remove(peer);
        }
        
        // Clean up old seen messages
        let mut seen = self.seen_messages.write().await;
        seen.retain(|_, timestamp| now.duration_since(*timestamp) < Duration::from_secs(300)); // 5 min TTL
    }
}

impl Default for P2pHandler {
    fn default() -> Self {
        Self::new()
    }
}
