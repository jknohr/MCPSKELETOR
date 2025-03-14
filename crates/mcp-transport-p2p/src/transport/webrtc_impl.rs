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
use libp2p_webrtc as webrtc;
use mcp_core::{Content, Resource};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// WebRTC transport implementation for MCP
pub struct WebRtcTransport {
    /// libp2p swarm for managing connections
    swarm: Arc<RwLock<libp2p::Swarm<P2pBehaviour>>>,
    /// Configuration
    config: Arc<P2pConfig>,
    /// Transport state
    state: Arc<RwLock<TransportState>>,
    /// Local peer ID
    local_peer_id: PeerId,
}

impl WebRtcTransport {
    /// Create a new WebRTC transport
    pub async fn new(config: P2pConfig) -> Result<Self, TransportError> {
        // Validate configuration
        config.validate().map_err(TransportError::Config)?;
        
        if !config.network.enable_webrtc {
            return Err(TransportError::Config(
                crate::config::ConfigError::Invalid("WebRTC is disabled in config".to_string())
            ));
        }

        // Generate identity
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        info!("WebRTC local peer id: {}", local_peer_id);

        // Create WebRTC transport with ICE servers from config
        let mut webrtc_config = webrtc::Config::new();
        
        // Add STUN servers from config
        for stun in &config.network.stun_servers {
            webrtc_config = webrtc_config.with_stun_server(stun.clone());
        }
        
        // Add TURN servers from config
        for turn in &config.network.turn_servers {
            webrtc_config = webrtc_config.with_turn_server(
                turn.url.clone(),
                turn.username.clone(),
                turn.credential.clone(),
            );
        }
        
        // Create transport
        let transport = webrtc::Transport::new(local_key.clone());

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

    /// Start the WebRTC transport
    pub async fn start(&self) -> Result<(), TransportError> {
        let mut swarm = self.swarm.write().await;

        // Start listening on the configured address
        swarm.listen_on(
            format!("/ip4/{}/udp/{}/webrtc-direct", 
                self.config.network.listen_addr.ip(),
                self.config.network.listen_addr.port()
            ).parse()
            .map_err(|e| TransportError::Init(format!("Failed to parse listen address: {}", e)))?
        )
        .map_err(|e| TransportError::Connection(e.to_string()))?;

        info!("WebRTC started on {}", self.config.network.listen_addr);
        
        // Connect to bootstrap peers if any
        for addr in &self.config.network.bootstrap_peers {
            match swarm.dial(addr.clone()) {
                Ok(_) => debug!("Dialing bootstrap peer: {}", addr),
                Err(e) => warn!("Failed to dial bootstrap peer {}: {}", addr, e),
            }
        }

        Ok(())
    }

    /// Send content to peers using the WebRTC protocol
    pub async fn send_content(&self, content: Content) -> Result<(), TransportError> {
        let mut swarm = self.swarm.write().await;

        // Use the P2P behavior to distribute content
        swarm.behaviour_mut().distribute_content(content)
            .await
            .map_err(|e| TransportError::Protocol(e.to_string()))?;

        Ok(())
    }

    /// Send resource to peers using the WebRTC protocol
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

impl crate::transport::P2pTransportExt for WebRtcTransport {
    fn get_peer_count(&self) -> usize {
        // In a real implementation, this would count the active peers
        0
    }

    fn is_connected(&self) -> bool {
        // In a real implementation, this would check if there are active peers
        true
    }
}
