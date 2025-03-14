use async_trait::async_trait;
use mcp_core::{Content, Resource};
use mcp_server::transport::McpTransport;
use std::{sync::Arc, any::Any};
use tokio::sync::RwLock;

// Import modules conditionally based on features
#[cfg(not(feature = "wasm"))]
mod webtransport_impl;
#[cfg(not(feature = "wasm"))]
mod webrtc_impl;

#[cfg(feature = "wasm")]
mod webtransport_websys_impl;
#[cfg(feature = "wasm")]
mod webrtc_websys_impl;

// Export the appropriate implementations based on feature flags
#[cfg(not(feature = "wasm"))]
pub use webtransport_impl::WebTransportP2p;
#[cfg(not(feature = "wasm"))]
pub use webrtc_impl::WebRtcTransport;

#[cfg(feature = "wasm")]
pub use webtransport_websys_impl::WebTransportP2p;
#[cfg(feature = "wasm")]
pub use webrtc_websys_impl::WebRtcTransport;

/// Common traits and utilities for P2P transports
pub(crate) trait P2pTransportExt {
    fn get_peer_count(&self) -> usize;
    fn is_connected(&self) -> bool;
}

#[async_trait]
impl McpTransport for WebTransportP2p {
    async fn send_content(&self, content: Content) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("No peers connected"));
        }
        self.send_content(content).await.map_err(|e| anyhow::anyhow!("Failed to send content: {}", e))
    }

    async fn send_resource(&self, resource: Resource) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("No peers connected"));
        }
        self.send_resource(resource).await.map_err(|e| anyhow::anyhow!("Failed to send resource: {}", e))
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[async_trait]
impl McpTransport for WebRtcTransport {
    async fn send_content(&self, content: Content) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("No peers connected"));
        }
        self.send_content(content).await.map_err(|e| anyhow::anyhow!("Failed to send content: {}", e))
    }

    async fn send_resource(&self, resource: Resource) -> anyhow::Result<()> {
        if !self.is_connected() {
            return Err(anyhow::anyhow!("No peers connected"));
        }
        self.send_resource(resource).await.map_err(|e| anyhow::anyhow!("Failed to send resource: {}", e))
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}
