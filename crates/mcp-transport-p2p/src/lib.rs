use async_trait::async_trait;
use libp2p::{
    core::transport::Transport,
    identity, PeerId,
    swarm::{Swarm, NetworkBehaviour},
    SwarmBuilder,
    ping,
};
use mcp_core::{Content, Resource};
use mcp_server::transport::McpTransport;
use std::{sync::Arc, any::Any};
use tokio::sync::RwLock;
use thiserror::Error;
use tracing::{debug, error, info, warn};

pub mod config;
pub mod protocol;
pub mod transport;

pub use config::{P2pConfig, ConfigError};
pub use transport::{WebTransportP2p, WebRtcTransport, P2pTransportExt};

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Transport initialization failed: {0}")]
    Init(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
}

/// Server state for P2P transport
#[derive(Debug, Clone, Default)]
pub struct TransportState {
    /// Active peer connections
    pub peers: Vec<PeerId>,
    /// Number of pending requests
    pub pending_requests: usize,
}

/// P2P Transport manager that handles both WebTransport and WebRTC connections
pub struct P2pTransportManager {
    /// Configuration
    config: Arc<P2pConfig>,
    /// WebTransport implementation (if enabled)
    web_transport: Option<Arc<WebTransportP2p>>,
    /// WebRTC implementation (if enabled)
    web_rtc: Option<Arc<WebRtcTransport>>,
    /// Transport state
    state: Arc<RwLock<TransportState>>,
}

impl P2pTransportManager {
    pub async fn new(config: P2pConfig) -> Result<Self, TransportError> {
        // Validate configuration
        config.validate().map_err(TransportError::Config)?;

        // Create transport implementations based on configuration
        let mut web_transport = None;
        let mut web_rtc = None;
        
        // Create WebTransport if enabled
        if config.network.enable_webtransport {
            let transport = transport::WebTransportP2p::new(config.clone()).await?;
            web_transport = Some(Arc::new(transport));
        }
        
        // Create WebRTC if enabled
        if config.network.enable_webrtc {
            let transport = transport::WebRtcTransport::new(config.clone()).await?;
            web_rtc = Some(Arc::new(transport));
        }
        
        // Ensure at least one transport is enabled
        if web_transport.is_none() && web_rtc.is_none() {
            return Err(TransportError::Init("No transport protocols enabled".to_string()));
        }

        let state = Arc::new(RwLock::new(TransportState::default()));
        
        Ok(Self {
            config: Arc::new(config),
            web_transport,
            web_rtc,
            state,
        })
    }

    pub async fn start(&self) -> Result<(), TransportError> {
        // Start WebTransport if enabled
        if let Some(transport) = &self.web_transport {
            transport.start().await?;
        }
        
        // Start WebRTC if enabled
        if let Some(transport) = &self.web_rtc {
            transport.start().await?;
        }

        info!("P2P transport started successfully");
        Ok(())
    }
    
    /// Shutdown the transport manager
    pub async fn shutdown(&self) -> Result<(), TransportError> {
        info!("Shutting down P2P transport");
        let mut state = self.state.write().await;
        state.peers.clear();
        state.pending_requests = 0;
        Ok(())
    }

    /// Check if transport is connected to peers
    pub async fn is_connected(&self) -> bool {
        let mut connected = false;
        
        // Check WebTransport
        if let Some(transport) = &self.web_transport {
            connected = connected || transport.is_connected();
        }
        
        // Check WebRTC
        if let Some(transport) = &self.web_rtc {
            connected = connected || transport.is_connected();
        }
        
        connected
    }
    
    /// Get peer count across all transports
    pub async fn get_peer_count(&self) -> usize {
        let mut count = 0;
        
        // Count WebTransport peers
        if let Some(transport) = &self.web_transport {
            count += transport.get_peer_count();
        }
        
        // Count WebRTC peers
        if let Some(transport) = &self.web_rtc {
            count += transport.get_peer_count();
        }
        
        count
    }
    
    /// Check resource limits
    async fn check_resource_limits(&self) -> Result<(), TransportError> {
        let state = self.state.read().await;
        if state.peers.len() >= self.config.resources.max_connections {
            return Err(TransportError::ResourceLimit("Max connections reached".to_string()));
        }
        if state.pending_requests >= self.config.resources.max_pending_requests {
            return Err(TransportError::ResourceLimit("Max pending requests reached".to_string()));
        }
        Ok(())
    }
    
    /// Send content via available transports
    pub async fn send_content(&self, content: Content) -> Result<(), TransportError> {
        self.check_resource_limits().await?;
        
        let mut state = self.state.write().await;
        state.pending_requests += 1;
        
        let result = self.send_content_internal(content.clone()).await;
        
        state.pending_requests -= 1;
        result
    }
    
    /// Internal implementation for sending content
    async fn send_content_internal(&self, content: Content) -> Result<(), TransportError> {
        if !self.is_connected().await {
            return Err(TransportError::Connection("No peers connected".to_string()));
        }
        
        let mut sent = false;
        
        // Try WebTransport first if available
        if let Some(transport) = &self.web_transport {
            match transport.send_content(content.clone()).await {
                Ok(_) => {
                    sent = true;
                }
                Err(e) => {
                    warn!("Failed to send content via WebTransport: {}", e);
                }
            }
        }
        
        // Try WebRTC if available and needed
        if !sent && self.web_rtc.is_some() {
            let transport = self.web_rtc.as_ref().unwrap();
            match transport.send_content(content).await {
                Ok(_) => {
                    sent = true;
                }
                Err(e) => {
                    warn!("Failed to send content via WebRTC: {}", e);
                }
            }
        }
        
        if sent {
            Ok(())
        } else {
            Err(TransportError::Protocol("Failed to send content via any transport".to_string()))
        }
    }
    
    /// Send resource via available transports
    pub async fn send_resource(&self, resource: Resource) -> Result<(), TransportError> {
        self.check_resource_limits().await?;
        
        let mut state = self.state.write().await;
        state.pending_requests += 1;
        
        let result = self.send_resource_internal(resource.clone()).await;
        
        state.pending_requests -= 1;
        result
    }
    
    /// Internal implementation for sending resource
    async fn send_resource_internal(&self, resource: Resource) -> Result<(), TransportError> {
        if !self.is_connected().await {
            return Err(TransportError::Connection("No peers connected".to_string()));
        }
        
        let mut sent = false;
        
        // Try WebTransport first if available
        if let Some(transport) = &self.web_transport {
            match transport.send_resource(resource.clone()).await {
                Ok(_) => {
                    sent = true;
                }
                Err(e) => {
                    warn!("Failed to send resource via WebTransport: {}", e);
                }
            }
        }
        
        // Try WebRTC if available and needed
        if !sent && self.web_rtc.is_some() {
            let transport = self.web_rtc.as_ref().unwrap();
            match transport.send_resource(resource).await {
                Ok(_) => {
                    sent = true;
                }
                Err(e) => {
                    warn!("Failed to send resource via WebRTC: {}", e);
                }
            }
        }
        
        if sent {
            Ok(())
        } else {
            Err(TransportError::Protocol("Failed to send resource via any transport".to_string()))
        }
    }
}

#[async_trait]
impl McpTransport for P2pTransportManager {
    async fn send_content(&self, content: Content) -> anyhow::Result<()> {
        self.send_content(content).await.map_err(|e| anyhow::anyhow!("{}", e))
    }

    async fn send_resource(&self, resource: Resource) -> anyhow::Result<()> {
        self.send_resource(resource).await.map_err(|e| anyhow::anyhow!("{}", e))
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}
