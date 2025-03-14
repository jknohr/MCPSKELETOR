use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Invalid(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

/// P2P transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pConfig {
    /// Network configuration
    pub network: NetworkConfig,
    /// Protocol settings
    pub protocol: ProtocolConfig,
    /// Resource limits
    pub resources: ResourceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listen address for P2P connections
    pub listen_addr: SocketAddr,
    /// External address for NAT traversal
    pub external_addr: Option<String>,
    /// Enable WebTransport protocol
    pub enable_webtransport: bool,
    /// Enable WebRTC protocol
    pub enable_webrtc: bool,
    /// STUN servers for WebRTC NAT traversal
    pub stun_servers: Vec<String>,
    /// TURN servers for WebRTC fallback
    pub turn_servers: Vec<TurnServer>,
    /// Bootstrap peers
    pub bootstrap_peers: Vec<Multiaddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnServer {
    pub url: String,
    pub username: String,
    pub credential: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version
    pub version: String,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Enable keepalive
    pub enable_keepalive: bool,
    /// Keepalive interval in seconds
    pub keepalive_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Maximum pending requests per connection
    pub max_pending_requests: usize,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig {
                listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 9090),
                external_addr: None,
                enable_webtransport: true,
                enable_webrtc: true,
                stun_servers: vec![
                    "stun.l.google.com:19302".to_string(),
                    "stun1.l.google.com:19302".to_string(),
                ],
                turn_servers: Vec::new(),
                bootstrap_peers: Vec::new(),
            },
            protocol: ProtocolConfig {
                version: env!("CARGO_PKG_VERSION").to_string(),
                max_message_size: 1024 * 1024, // 1MB
                connection_timeout: 30,
                enable_keepalive: true,
                keepalive_interval: 30,
            },
            resources: ResourceConfig {
                max_connections: 1000,
                max_pending_requests: 100,
            },
        }
    }
}

impl P2pConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate network configuration
        if self.network.enable_webrtc && self.network.stun_servers.is_empty() {
            return Err(ConfigError::Invalid(
                "WebRTC enabled but no STUN servers configured".to_string(),
            ));
        }

        // Validate protocol configuration
        if self.protocol.max_message_size == 0 {
            return Err(ConfigError::Invalid(
                "Maximum message size cannot be zero".to_string(),
            ));
        }

        if self.protocol.connection_timeout == 0 {
            return Err(ConfigError::Invalid(
                "Connection timeout cannot be zero".to_string(),
            ));
        }

        // Validate resource limits
        if self.resources.max_connections == 0 {
            return Err(ConfigError::Invalid(
                "Maximum connections cannot be zero".to_string(),
            ));
        }

        if self.resources.max_pending_requests == 0 {
            return Err(ConfigError::Invalid(
                "Maximum pending requests cannot be zero".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = P2pConfig::default();
        assert!(config.validate().is_ok());
        assert!(config.network.enable_webtransport);
        assert!(config.network.enable_webrtc);
        assert!(!config.network.stun_servers.is_empty());
    }

    #[test]
    fn test_invalid_config() {
        let mut config = P2pConfig::default();
        config.resources.max_connections = 0;
        assert!(config.validate().is_err());
    }
}
