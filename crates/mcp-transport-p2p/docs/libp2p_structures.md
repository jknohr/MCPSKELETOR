# libp2p 0.55.0 Structure Reference

This document provides a reference for the key structures and events in libp2p 0.55.0 that are used in the MCP P2P transport implementation. Use this as a guide when implementing pattern matching for these structures.

## Core Structures

### Resource (mcp_core)

```rust
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: String,
    pub annotations: Option<Annotations>,
}
```

### Content (mcp_core)

```rust
pub struct Content {
    pub uri: String,
    pub data: Vec<u8>,
    pub mime_type: String,
}
```

## Event Structures

### Ping Events

```rust
ping::Event {
    peer: PeerId,
    connection: ConnectionId,
    result: Result<Duration, PingFailure>,
}
```

### Identify Events

```rust
identify::Event::Received {
    peer_id: PeerId,
    info: Info,
    connection_id: ConnectionId,
}

identify::Event::Sent {
    peer_id: PeerId,
    connection_id: ConnectionId,
}

identify::Event::Error {
    peer_id: PeerId,
    error: Error,
    connection_id: ConnectionId,
}
```

### Request-Response Events

```rust
request_response::Event::Message {
    peer: PeerId,
    message: Message<TRequest, TResponse>,
    connection_id: ConnectionId,
}

request_response::Message::Request {
    request: TRequest,
    channel: ResponseChannel<TResponse>,
    request_id: RequestId,
}

request_response::Message::Response {
    request_id: RequestId,
    response: TResponse,
}

request_response::Event::OutboundFailure {
    peer: PeerId,
    request_id: RequestId,
    error: OutboundFailure,
    connection_id: ConnectionId,
}

request_response::Event::InboundFailure {
    peer: PeerId,
    error: InboundFailure,
    connection_id: ConnectionId,
    request_id: RequestId,
}

request_response::Event::ResponseSent {
    peer: PeerId,
    connection_id: ConnectionId,
    request_id: RequestId,
}
```

### Gossipsub Events

```rust
gossipsub::Event::Message {
    propagation_source: PeerId,
    message_id: MessageId,
    message: Message,
}

gossipsub::Event::Subscribed {
    peer_id: PeerId,
    topic: TopicHash,
}

gossipsub::Event::Unsubscribed {
    peer_id: PeerId,
    topic: TopicHash,
}

gossipsub::Event::GossipsubNotSupported {
    peer_id: PeerId,
}
```

## Transport Configuration

### WebTransport Crate Setup

WebTransport is not a direct feature in libp2p 0.55.0 and needs to be configured as:

```toml
[features]
webtransport = ["libp2p-webtransport-websys"]

[dependencies]
libp2p-webtransport-websys = { version = "0.5.0", optional = true }
```

### WebRTC Crate Setup

WebRTC is not a direct feature in libp2p 0.55.0 and needs to be configured as:

```toml
[features]
webrtc = ["libp2p-webrtc"]

[dependencies]
libp2p-webrtc = { version = "0.5.0", optional = true }
```

## Conditional Compilation

Example for conditionally implementing WebTransport and WebRTC support:

```rust
#[cfg(feature = "webtransport")]
mod webtransport_impl {
    // WebTransport-specific implementation
}

#[cfg(feature = "webrtc")]
mod webrtc_impl {
    // WebRTC-specific implementation
}
```

## Request and Response Handling

When handling request-response protocol messages, note that libp2p 0.55.0 uses `send_response` instead of `respond`:

```rust
// Proper way to send a response
if let Err(e) = request_response_behaviour.send_response(channel, response_data) {
    error!("Failed to send response: {:?}", e);
}
```

## Common Patterns

### Dispatching P2P Behavior Events

```rust
match event {
    P2pBehaviourEvent::Ping(ping_event) => {
        // Handle ping events
    },
    P2pBehaviourEvent::Identify(identify_event) => {
        // Handle identify events
    },
    P2pBehaviourEvent::RequestResponse(req_res_event) => {
        // Handle request-response events
    },
    P2pBehaviourEvent::Gossipsub(gossipsub_event) => {
        // Handle gossipsub events
    },
}
```
