use crate::{
    config::P2pConfig, 
    protocol::{P2pBehaviour, McpMessage},
    TransportState,
    TransportError
};
use async_trait::async_trait;
use libp2p::{
    core::transport::Transport,
    identity::{self, Keypair},
    swarm::{SwarmBuilder, NetworkBehaviour},
    PeerId,
};
use libp2p_webtransport as webtransport;
use mcp_core::{Content, Resource};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// WebTransport implementation for MCP
pub struct WebTransportP2p {
    /// libp2p swarm for managing connections
    swarm: Arc<RwLock<libp2p::Swarm<P2pBehaviour>>>,
    /// Configuration
    config: Arc<P2pConfig>,
    /// Transport state
    state: Arc<RwLock<TransportState>>,
    /// Local peer ID
    local_peer_id: PeerId,
}

impl WebTransportP2p {
    /// Create a new WebTransport P2P transport
    pub async fn new(config: P2pConfig) -> Result<Self, TransportError> {
        // Validate configuration
        config.validate().map_err(TransportError::Config)?;
        
        if !config.network.enable_webtransport {
            return Err(TransportError::Config(
                crate::config::ConfigError::Invalid("WebTransport is disabled in config".to_string())
            ));
        }

        // Generate identity
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        info!("WebTransport local peer id: {}", local_peer_id);

        // Create transport
        let transport = webtransport::Transport::new(local_key.clone());

        // Create behavior
        let behaviour = P2pBehaviour::new(config.clone()).await
            .map_err(|e| TransportError::Init(e.to_string()))?;

        // Create swarm
        let swarm = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id).build();

        let config = Arc::new(config);
        
        Ok(Self {
            swarm: Arc::new(RwLock::new(swarm)),
            config,
            state: Arc::new(RwLock::new(TransportState {
                peers: Vec::new(),
                pending_requests: 0,
            })),
            local_peer_id,
        })
    }

    /// Start the WebTransport transport
    pub async fn start(&self) -> Result<(), TransportError> {
        let mut swarm = self.swarm.write().await;

        // Start listening
        swarm.listen_on(
            format!("/ip4/{}/tcp/{}/webtransport", 
                self.config.network.listen_addr.ip(),
                self.config.network.listen_addr.port()
            ).parse()
            .map_err(|e| TransportError::Init(format!("Failed to parse listen address: {}", e)))?
        )
        .map_err(|e| TransportError::Connection(e.to_string()))?;

        info!("WebTransport started on {}", self.config.network.listen_addr);
        Ok(())
    }

    /// Send content to peers using the WebTransport protocol
    pub async fn send_content(&self, content: Content) -> Result<(), TransportError> {
        let mut swarm = self.swarm.write().await;

        // Use the P2P behavior to distribute content
        swarm.behaviour_mut().distribute_content(content)
            .await
            .map_err(|e| TransportError::Protocol(e.to_string()))?;

        Ok(())
    }

    /// Send resource to peers using the WebTransport protocol
    pub async fn send_resource(&self, resource: Resource) -> Result<(), TransportError> {
        let mut swarm = self.swarm.write().await;

        // Use the P2P behavior to distribute resource
        swarm.behaviour_mut().distribute_resource(resource)
            .await
            .map_err(|e| TransportError::Protocol(e.to_string()))?;

        Ok(())
    }

    /// Get the local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }
}

impl crate::transport::P2pTransportExt for WebTransportP2p {
    fn get_peer_count(&self) -> usize {
        // In a real implementation, this would count the active peers
        0
    }

    fn is_connected(&self) -> bool {
        // In a real implementation, this would check if there are active peers
        true
    }
}
