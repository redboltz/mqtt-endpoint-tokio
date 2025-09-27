# mqtt-endpoint-tokio

[![Crates.io](https://img.shields.io/crates/v/mqtt-endpoint-tokio.svg)](https://crates.io/crates/mqtt-endpoint-tokio)
[![Documentation](https://docs.rs/mqtt-endpoint-tokio/badge.svg)](https://docs.rs/mqtt-endpoint-tokio)
[![CI](https://github.com/redboltz/mqtt-endpoint-tokio/workflows/CI/badge.svg)](https://github.com/redboltz/mqtt-endpoint-tokio/actions)
[![codecov](https://codecov.io/gh/redboltz/mqtt-endpoint-tokio/branch/main/graph/badge.svg)](https://codecov.io/gh/redboltz/mqtt-endpoint-tokio)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance async MQTT client/server library for Rust with tokio, supporting MQTT v5.0 and v3.1.1 with TCP, TLS, and WebSocket transports.

## Features

- **Full MQTT Protocol Support**: MQTT v5.0 and v3.1.1 compatible
- **Multiple Transport Layers**: TCP, TLS, and WebSocket support
- **Async/Await**: Built on tokio for high-performance async I/O
- **Generic Packet ID Support**: Supports both u16 and u32 packet IDs for broker clustering
- **Sans-I/O Protocol Core**: Uses [mqtt-protocol-core](https://crates.io/crates/mqtt-protocol-core) for protocol implementation
- **Type Safety**: Comprehensive type system for MQTT packet handling
- **Connection Management**: Robust connection lifecycle management
- **Error Handling**: Comprehensive error types for all failure modes

## Transport Support

### TCP Transport
Basic TCP connections for standard MQTT communication.

### TLS Transport
Secure connections with full TLS support using rustls.

### WebSocket Transport
WebSocket connections for web-based MQTT clients, supporting both:
- Plain WebSocket (ws://)
- Secure WebSocket over TLS (wss://)

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
mqtt-endpoint-tokio = "0.5"
```

### Basic Client Example

```rust
use mqtt_endpoint_tokio::mqtt_ep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create endpoint
    let endpoint = mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);

    // Connect to broker
    let tcp_stream = mqtt_ep::transport::connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    let transport = mqtt_ep::transport::TcpTransport::from_stream(tcp_stream);
    endpoint.attach(transport, mqtt_ep::endpoint::Mode::Client).await?;

    // Send CONNECT packet
    let connect = mqtt_ep::packet::v5_0::Connect::builder()
        .client_id("rust-client")
        .unwrap()
        .keep_alive(60)
        .clean_start(true)
        .build()
        .unwrap();

    endpoint.send(connect).await?;

    // Receive CONNACK
    let packet = endpoint.recv().await?;
    println!("Received: {packet:?}");

    Ok(())
}
```

### WebSocket Example

```rust
use mqtt_endpoint_tokio::mqtt_ep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect via WebSocket
    let ws_stream = mqtt_ep::transport::connect_helper::connect_tcp_ws(
        "127.0.0.1:8080",
        "/mqtt",
        None,
        None
    ).await?;

    let transport = mqtt_ep::transport::WebSocketTransport::from_tcp_client_stream(ws_stream.into_inner());
    // ... rest of MQTT communication

    Ok(())
}
```

### TLS Example

```rust
use mqtt_endpoint_tokio::mqtt_ep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect via TLS
    let tls_stream = mqtt_ep::transport::connect_helper::connect_tcp_tls(
        "broker.example.com:8883",
        "broker.example.com",
        None,
        None
    ).await?;

    let transport = mqtt_ep::transport::TlsTransport::from_stream(tls_stream);
    // ... rest of MQTT communication

    Ok(())
}
```

## Architecture

This library is built on top of [mqtt-protocol-core](https://crates.io/crates/mqtt-protocol-core), which provides the Sans-I/O MQTT protocol implementation. The `mqtt-endpoint-tokio` layer adds:

- Tokio-based async I/O
- Transport layer abstractions
- Connection management
- Error handling integration

## Generic Packet ID Support

The library supports both standard u16 packet IDs and extended u32 packet IDs for broker clustering scenarios:

```rust
use mqtt_endpoint_tokio::mqtt_ep;

// Standard u16 packet IDs
type StandardEndpoint = mqtt_ep::endpoint::Endpoint<mqtt_ep::role::Client>;

// Extended u32 packet IDs for broker clustering
type ExtendedEndpoint = mqtt_ep::endpoint::GenericEndpoint<mqtt_ep::role::Client, u32>;
```

## Error Handling

Comprehensive error types provide detailed information about failures:

```rust
use mqtt_endpoint_tokio::mqtt_ep;

match endpoint.send(packet).await {
    Ok(()) => println!("Packet sent"),
    Err(mqtt_ep::connection_error::ConnectionError::NotConnected) => println!("Need to connect first"),
    Err(mqtt_ep::connection_error::ConnectionError::Transport(e)) => println!("Network error: {e}"),
    Err(mqtt_ep::connection_error::ConnectionError::Mqtt(e)) => println!("Protocol error: {e:?}"),
    Err(e) => println!("Other error: {e}"),
}
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
