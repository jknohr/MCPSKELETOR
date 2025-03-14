use libp2p::Multiaddr;
use mcp_core::{Content, Resource, TextContent, Role};
use mcp_server::transport::McpTransport;
use mcp_transport_p2p::{
    P2pConfig, P2pTransportManager,
    config::{NetworkConfig, ProtocolConfig, ResourceConfig},
};
use std::error::Error;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;
use tracing::{info, error};

/// Initialize the tracing subscriber for logging
fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};
    
    fmt::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive("p2p_server=debug".parse().unwrap())
            .add_directive("mcp_transport_p2p=debug".parse().unwrap()))
        .with_target(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    init_tracing();
    
    // Create P2P transport configuration
    let config = P2pConfig {
        network: NetworkConfig {
            listen_addr: "127.0.0.1:9090".parse()?,
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
            version: "1.0.0".to_string(),
            max_message_size: 1024 * 1024, // 1MB
            connection_timeout: 30,
            enable_keepalive: true,
            keepalive_interval: 30,
        },
        resources: ResourceConfig {
            max_connections: 100,
            max_pending_requests: 50,
        },
    };
    
    // Create and start P2P transport manager
    info!("Initializing P2P transport manager...");
    let transport = P2pTransportManager::new(config).await?;
    transport.start().await?;
    
    // Create a shutdown signal handler
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    
    // Spawn a task to handle Ctrl+C
    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        info!("Received shutdown signal");
        let _ = shutdown_tx.send(()).await;
    });
    
    // Simulate sending content periodically
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Create sample content
                    let content = Content::Text(TextContent {
                        text: "Hello from P2P MCP server!".to_string(),
                        role: Role::Assistant,
                        annotations: None,
                    });
                    
                    match transport.send_content(content).await {
                        Ok(_) => info!("Content sent successfully"),
                        Err(e) => error!("Failed to send content: {}", e),
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutting down content sender");
                    break;
                }
            }
        }
    });
    
    // Keep the main thread running until shutdown
    info!("P2P MCP server running on 127.0.0.1:9090");
    info!("Press Ctrl+C to shutdown");
    
    // Wait for Ctrl+C
    let _ = signal::ctrl_c().await;
    info!("Server shutting down...");
    
    Ok(())
}
