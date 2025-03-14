use mcp_core::{Content, Resource, TextContent, Role, ResourceContents};
use mcp_server::transport::McpTransport;
use mcp_transport_p2p::{
    P2pConfig, P2pTransportManager,
    config::{NetworkConfig, ProtocolConfig, ResourceConfig},
    transport::{WebTransportP2p, WebRtcTransport},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::timeout;
use std::time::Duration;

// Test helper to create a default P2P config for testing
fn create_test_config() -> P2pConfig {
    P2pConfig {
        network: NetworkConfig {
            listen_addr: "127.0.0.1:0".parse().unwrap(), // Random port
            external_addr: None,
            enable_webtransport: true,
            enable_webrtc: true,
            stun_servers: vec![
                "stun.l.google.com:19302".to_string(),
            ],
            turn_servers: Vec::new(),
            bootstrap_peers: Vec::new(),
        },
        protocol: ProtocolConfig {
            version: "1.0.0".to_string(),
            max_message_size: 1024 * 1024, // 1MB
            connection_timeout: 5,
            enable_keepalive: true,
            keepalive_interval: 5,
        },
        resources: ResourceConfig {
            max_connections: 10,
            max_pending_requests: 5,
        },
    }
}

#[tokio::test]
async fn test_transport_manager_creation() {
    let config = create_test_config();
    let result = P2pTransportManager::new(config).await;
    assert!(result.is_ok(), "Failed to create P2P transport manager");
}

#[tokio::test]
async fn test_transport_manager_start() {
    let config = create_test_config();
    let transport = P2pTransportManager::new(config).await.unwrap();
    let result = transport.start().await;
    assert!(result.is_ok(), "Failed to start P2P transport manager");
}

#[tokio::test]
async fn test_send_content() {
    let config = create_test_config();
    let transport = P2pTransportManager::new(config).await.unwrap();
    transport.start().await.unwrap();
    
    let content = Content::Text(TextContent {
        text: "Test content".to_string(),
        role: Role::Assistant,
        annotations: None,
    });
    
    let result = transport.send_content(content).await;
    assert!(result.is_ok(), "Failed to send content: {:?}", result);
}

#[tokio::test]
async fn test_webtransport_creation() {
    let mut config = create_test_config();
    config.network.enable_webtransport = true;
    config.network.enable_webrtc = false;
    
    let result = WebTransportP2p::new(config).await;
    assert!(result.is_ok(), "Failed to create WebTransport: {:?}", result);
}

#[tokio::test]
async fn test_webrtc_creation() {
    let mut config = create_test_config();
    config.network.enable_webtransport = false;
    config.network.enable_webrtc = true;
    
    let result = WebRtcTransport::new(config).await;
    assert!(result.is_ok(), "Failed to create WebRTC transport: {:?}", result);
}

#[tokio::test]
async fn test_invalid_config() {
    // Test with invalid configuration (both transports disabled)
    let mut config = create_test_config();
    config.network.enable_webtransport = false;
    config.network.enable_webrtc = false;
    
    let result = P2pTransportManager::new(config).await;
    assert!(result.is_err(), "Expected error with invalid config but got success");
}

#[tokio::test]
async fn test_resource_limits() {
    let mut config = create_test_config();
    config.resources.max_pending_requests = 1;
    
    let transport = P2pTransportManager::new(config).await.unwrap();
    transport.start().await.unwrap();
    
    // Send resources in quick succession
    let resource1 = Resource {
        id: "test1".to_string(),
        contents: ResourceContents::Text("Test resource 1".to_string()),
    };
    
    let resource2 = Resource {
        id: "test2".to_string(),
        contents: ResourceContents::Text("Test resource 2".to_string()),
    };
    
    // First request should succeed
    let result1 = transport.send_resource(resource1).await;
    assert!(result1.is_ok(), "First resource send failed");
    
    // Allow the first request to be processed
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Second request should also succeed since the first one should be done
    let result2 = transport.send_resource(resource2).await;
    assert!(result2.is_ok(), "Second resource send failed");
}
