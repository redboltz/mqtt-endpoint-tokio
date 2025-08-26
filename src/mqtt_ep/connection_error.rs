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

use crate::mqtt_ep::result_code::MqttError;
use crate::mqtt_ep::transport::TransportError;

/// Unified Error Type for MQTT Endpoint Operations
///
/// This enum represents all possible errors that can occur during MQTT endpoint operations,
/// providing a comprehensive error handling mechanism that covers both protocol-level and
/// transport-level failures. The error type is designed to be informative while maintaining
/// simplicity for application developers.
///
/// # Error Categories
///
/// The errors are categorized into several types based on their origin and nature:
///
/// - **Protocol Errors**: MQTT specification violations and protocol-level issues
/// - **Transport Errors**: Network, I/O, and connection-level failures
/// - **State Errors**: Invalid operations based on current endpoint state
/// - **Internal Errors**: Framework-level communication failures
///
/// # Error Hierarchy
///
/// ```text
/// ConnectionError
/// ├── Mqtt(MqttError)           - Protocol-level errors from mqtt-protocol-core
/// ├── Transport(TransportError) - I/O and network errors
/// ├── ChannelClosed            - Internal communication failure
/// ├── NotConnected             - Operation requires active connection
/// ├── AlreadyConnected         - Connection already established
/// └── AlreadyDisconnected      - Connection already closed
/// ```
///
/// # Usage Patterns
///
/// Most MQTT endpoint operations return `Result<T, ConnectionError>`, allowing applications
/// to handle errors appropriately:
///
/// ```ignore
/// use mqtt_endpoint_tokio::mqtt_ep::{ConnectionError, GenericEndpoint};
///
/// async fn handle_mqtt_operation() -> Result<(), ConnectionError> {
///     let endpoint = GenericEndpoint::new(Version::V5_0);
///
///     // Operations can fail with various error types
///     match endpoint.send(publish_packet).await {
///         Ok(()) => println!("Message sent successfully"),
///         Err(ConnectionError::NotConnected) => {
///             println!("Need to establish connection first");
///         },
///         Err(ConnectionError::Transport(transport_err)) => {
///             println!("Network error: {transport_err}");
///         },
///         Err(ConnectionError::Mqtt(mqtt_err)) => {
///             println!("MQTT protocol error: {mqtt_err:?}");
///         },
///         Err(e) => {
///             println!("Other error: {e}");
///         }
///     }
///     Ok(())
/// }
/// ```
///
/// # Error Conversion
///
/// The error type implements automatic conversion from underlying error types:
/// - `MqttError` → `ConnectionError::Mqtt`
/// - `TransportError` → `ConnectionError::Transport`
///
/// This allows seamless error propagation through the `?` operator.
///
/// # Error Context
///
/// Each error variant provides specific context about the failure:
///
/// - **Mqtt**: Contains detailed MQTT protocol error information
/// - **Transport**: Includes network/I/O error details
/// - **State errors**: Indicate the current state that prevents the operation
/// - **ChannelClosed**: Indicates internal framework communication failure
///
/// # Thread Safety
///
/// This error type is `Send + Sync`, making it suitable for use across async task boundaries
/// and multi-threaded environments.
#[derive(Debug)]
pub enum ConnectionError {
    /// MQTT protocol-level error from mqtt-protocol-core
    ///
    /// This variant wraps errors that originate from the MQTT protocol implementation,
    /// including packet parsing failures, protocol violations, and specification
    /// compliance issues. These errors typically indicate problems with MQTT
    /// message format, invalid packet sequences, or protocol state violations.
    ///
    /// # Common Causes
    /// - Malformed MQTT packets received from peer
    /// - Invalid packet sequences (e.g., PUBLISH without prior CONNECT)
    /// - Protocol version mismatches
    /// - QoS level violations
    /// - Topic name validation failures
    /// - Property validation errors (MQTT v5.0)
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::Mqtt(mqtt_err) => {
    ///         println!("MQTT protocol error: {mqtt_err:?}");
    ///         // Handle protocol-specific error recovery
    ///     }
    ///     _ => {}
    /// }
    /// ```
    Mqtt(MqttError),

    /// I/O or transport-level error
    ///
    /// This variant encompasses all network and transport-related failures that
    /// occur before MQTT protocol processing. These errors originate from the
    /// underlying transport layer (TCP, TLS, WebSocket) and typically indicate
    /// connectivity issues, network failures, or transport configuration problems.
    ///
    /// # Common Causes
    /// - Network connectivity failures
    /// - TCP connection errors (refused, timeout, reset)
    /// - TLS handshake failures or certificate issues
    /// - WebSocket upgrade failures
    /// - DNS resolution failures
    /// - Firewall or proxy blocking
    /// - Transport-specific configuration errors
    ///
    /// # Recovery Strategies
    /// Transport errors often require connection re-establishment or
    /// configuration changes to resolve.
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::Transport(transport_err) => {
    ///         println!("Network error: {transport_err}");
    ///         // Implement retry logic or connection recovery
    ///     }
    ///     _ => {}
    /// }
    /// ```
    Transport(TransportError),

    /// Internal channel communication error
    ///
    /// This variant indicates a failure in internal communication channels between
    /// the endpoint's public API and its background processing task. This is typically
    /// an internal framework error that suggests the endpoint's background task
    /// has stopped unexpectedly or the internal message passing system has failed.
    ///
    /// # Common Causes
    /// - Endpoint background task panic or unexpected termination
    /// - Internal tokio channel closure
    /// - Resource exhaustion preventing message passing
    /// - Framework-level bugs or race conditions
    ///
    /// # Recovery
    /// This error usually indicates a serious internal problem. Applications
    /// should typically recreate the endpoint instance rather than attempting
    /// to recover from this error state.
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::ChannelClosed => {
    ///         println!("Internal communication failure - recreating endpoint");
    ///         // Recreate endpoint instance
    ///     }
    ///     _ => {}
    /// }
    /// ```
    ChannelClosed,

    /// Endpoint is not connected to any transport
    ///
    /// This error occurs when attempting operations that require an active
    /// transport connection, but the endpoint is not currently connected to
    /// any transport. The endpoint must be attached to a transport using
    /// `attach()` or `attach_with_options()` before performing most operations.
    ///
    /// # Affected Operations
    /// Most endpoint operations require an active connection:
    /// - `send()` - Sending MQTT packets
    /// - `recv()` / `recv_filtered()` - Receiving MQTT packets
    /// - `acquire_packet_id()` - Packet ID management
    /// - Protocol-specific operations
    ///
    /// # Resolution
    /// Call `attach()` or `attach_with_options()` with a valid transport
    /// before attempting these operations.
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::NotConnected => {
    ///         println!("Need to establish connection first");
    ///         // Call endpoint.attach(transport).await?
    ///     }
    ///     _ => {}
    /// }
    /// ```
    NotConnected,

    /// Connection is already established
    ///
    /// This error occurs when attempting to establish a connection using
    /// `attach()` or `attach_with_options()` while the endpoint is already
    /// connected to a transport. Each endpoint instance can only maintain
    /// one active connection at a time.
    ///
    /// # Resolution Strategies
    /// 1. Use the existing connection if appropriate
    /// 2. Call `close()` to disconnect, then attach new transport
    /// 3. Create a new endpoint instance for additional connections
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::AlreadyConnected => {
    ///         println!("Connection already active");
    ///         // Either use existing connection or close() then reconnect
    ///     }
    ///     _ => {}
    /// }
    /// ```
    AlreadyConnected,

    /// Connection is already closed
    ///
    /// This error occurs when attempting to close an endpoint connection
    /// that has already been closed. The endpoint is already in the
    /// disconnected state and no further close operation is needed.
    ///
    /// # Context
    /// This is typically a benign error that indicates the application
    /// is attempting redundant cleanup. It may occur in error handling
    /// paths or during application shutdown when multiple code paths
    /// attempt to close the same connection.
    ///
    /// # Handling
    /// This error can usually be safely ignored as it indicates the
    /// desired state (disconnected) has already been achieved.
    ///
    /// # Examples
    /// ```ignore
    /// match error {
    ///     ConnectionError::AlreadyDisconnected => {
    ///         println!("Connection already closed - no action needed");
    ///         // This is typically safe to ignore
    ///     }
    ///     _ => {}
    /// }
    /// ```
    AlreadyDisconnected,
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionError::Mqtt(e) => write!(f, "MQTT protocol error: {e:?}"),
            ConnectionError::Transport(e) => write!(f, "Transport error: {e}"),
            ConnectionError::ChannelClosed => write!(f, "Internal channel closed"),
            ConnectionError::NotConnected => write!(f, "Not connected"),
            ConnectionError::AlreadyConnected => write!(f, "Already connected"),
            ConnectionError::AlreadyDisconnected => write!(f, "Already disconnected"),
        }
    }
}

impl std::error::Error for ConnectionError {}

/// Automatic conversion from MQTT protocol errors
///
/// This implementation allows `MqttError` instances to be automatically
/// converted to `ConnectionError::Mqtt` variants, enabling seamless
/// error propagation from the mqtt-protocol-core layer.
///
/// # Usage
/// ```ignore
/// fn mqtt_operation() -> Result<(), ConnectionError> {
///     // MqttError is automatically converted via ?
///     some_mqtt_operation()?;
///     Ok(())
/// }
/// ```
impl From<MqttError> for ConnectionError {
    fn from(e: MqttError) -> Self {
        ConnectionError::Mqtt(e)
    }
}

/// Automatic conversion from transport errors
///
/// This implementation allows `TransportError` instances to be automatically
/// converted to `ConnectionError::Transport` variants, enabling seamless
/// error propagation from the transport layer.
///
/// # Usage
/// ```ignore
/// fn transport_operation() -> Result<(), ConnectionError> {
///     // TransportError is automatically converted via ?
///     some_transport_operation()?;
///     Ok(())
/// }
/// ```
impl From<TransportError> for ConnectionError {
    fn from(e: TransportError) -> Self {
        ConnectionError::Transport(e)
    }
}
