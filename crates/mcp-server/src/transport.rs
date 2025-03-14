use async_trait::async_trait;
use anyhow::Result;
use mcp_core::{Content, Resource};
use std::any::Any;

/// Transport trait for MCP Server
/// 
/// This trait defines the interface for different transport implementations
/// to integrate with the MCP Server.
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Send content via this transport
    fn send_content(&self, content: Content) -> Result<()>;

    /// Send resource via this transport
    fn send_resource(&self, resource: Resource) -> Result<()>;

    /// Get this transport as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}
