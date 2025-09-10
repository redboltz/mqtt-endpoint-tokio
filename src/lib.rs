// MIT License
//
// Copyright (c) 2025 Takatoshi Kondo
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! # MQTT Endpoint Tokio
//!
//! A high-performance async MQTT client/server library for Rust with tokio, supporting
//! MQTT v5.0 and v3.1.1 with TCP, TLS, and WebSocket transports.
//!
//! This library provides a Sans-I/O MQTT protocol implementation built on top of
//! `mqtt-protocol-core`, with async I/O operations handled by tokio.
//!
//! ## Features
//!
//! - **MQTT Protocol Support**: Both MQTT v3.1.1 and v5.0
//! - **Multiple Transports**: TCP, TLS, and WebSocket
//! - **Generic Packet ID Types**: Support for u16 and u32 packet IDs for broker clustering
//! - **Client and Server Roles**: Both client and server endpoint implementations
//! - **Async/Await**: Built on tokio for high-performance async I/O
//! - **Type Safety**: Comprehensive type system for MQTT packet handling
//!
//! ## Quick Start
//!
//! ```ignore
//! use mqtt_endpoint_tokio::mqtt_ep;
//!
//! // Create a client endpoint
//! let endpoint = mqtt_ep::endpoint::Endpoint::new(mqtt_ep::Version::V5_0);
//!
//! // Connect to TCP transport
//! let transport = mqtt_ep::transport::tcp::TcpTransport::new("localhost:1883").await?;
//! endpoint.attach(transport).await?;
//!
//! // Send CONNECT packet
//! let connect = mqtt_ep::packet::Connect::builder()
//!     .client_id("my-client")
//!     .build()?;
//! endpoint.send(connect.into()).await?;
//!
//! // Receive CONNACK
//! let packet = endpoint.recv().await?;
//! println!("Received: {packet:?}");
//! ```
//!
//! ## Main Components
//!
//! - [`mqtt_ep::endpoint`]: Core endpoint functionality for both client and server
//! - [`mqtt_ep::transport`]: Transport layer implementations (TCP, TLS, WebSocket)
//! - [`mqtt_ep::connection_option`]: Configuration options for connection behavior
//! - [`mqtt_ep::packet`]: MQTT packet types and builders
//! - [`mqtt_ep::packet_filter`]: Packet filtering for selective message reception
//! - [`mqtt_ep::connection_error`]: Error handling for connection operations
//!
//! ## Generic Packet ID Support
//!
//! The library supports generic packet ID types (u16, u32) for broker clustering scenarios:
//!
//! ```ignore
//! // Standard u16 packet IDs
//! type Endpoint = mqtt_ep::endpoint::GenericEndpoint<mqtt_ep::role::Client, u16>;
//!
//! // Extended u32 packet IDs for broker clustering
//! type ExtendedEndpoint = mqtt_ep::endpoint::GenericEndpoint<mqtt_ep::role::Client, u32>;
//! ```

pub mod mqtt_ep;
