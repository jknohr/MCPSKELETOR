# rust-sdk
The official Rust SDK for the Model Context Protocol

## SDK Structure

The SDK is organized into several core crates, each with specific responsibilities:

### Core Components (`mcp-core`)
- **Content Management**: Handles different content types (text, images) and annotations
- **Protocol**: Core protocol definitions and implementations
- **Tools**: Tool definition and execution infrastructure
- **Resources**: Resource management and content handling
- **Roles**: Role-based content attribution (User/Assistant)
- **Handlers**: Tool execution and error handling

### Client Implementation (`mcp-client`)
- **Client Core**: Main client implementation with capabilities management
- **Transport Layer**: Multiple transport implementations
  - SSE (Server-Sent Events) Transport
  - Standard I/O Transport
- **Service Interface**: Client service definitions

### Server Implementation (`mcp-server`)
- Server-side protocol handling
- Request processing and routing
- Connection management

### Macro Support (`mcp-macros`)
- Procedural macros for SDK functionality
- Code generation utilities

## Examples
The SDK includes example implementations:
- Client examples
- Server examples
- Macro usage examples

## Project Layout
```
crates/
├── mcp-client/    # Client implementation
├── mcp-core/      # Core protocol and types
├── mcp-macros/    # Procedural macros
└── mcp-server/    # Server implementation

examples/
├── clients/       # Client examples
├── servers/       # Server examples
└── macros/        # Macro usage examples
