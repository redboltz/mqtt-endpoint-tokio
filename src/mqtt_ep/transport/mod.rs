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

//! Transport layer implementations for MQTT connections.
//!
//! This module provides transport abstractions and implementations for MQTT protocol
//! communication. It includes built-in support for TCP, TLS, and WebSocket transports,
//! and allows users to implement custom transport layers.
//!
//! # Built-in Transports
//!
//! - **TCP**: Basic TCP socket transport for unencrypted connections
//! - **TLS**: TLS-encrypted TCP transport for secure connections
//! - **WebSocket**: WebSocket transport for web-based connections
//! - **WebSocket over TLS**: Secure WebSocket transport
//! - **QUIC**: QUIC transport for modern, secure, and efficient connections
//!
//! # Custom Transport Implementation
//!
//! Users can implement their own transport by implementing the [`TransportOps`] trait.
//! This allows integration with custom network protocols, specialized hardware,
//! or other transport mechanisms not covered by the built-in implementations.

pub mod connect_helper;
#[cfg(feature = "quic")]
mod quic;
mod tcp;
#[cfg(feature = "tls")]
mod tls;
#[cfg(all(feature = "unix-socket", unix))]
mod unix;
#[cfg(feature = "ws")]
mod websocket;

#[cfg(feature = "quic")]
pub use quic::QuicTransport;
pub use tcp::TcpTransport;
#[cfg(feature = "tls")]
pub use tls::TlsTransport;
#[cfg(all(feature = "unix-socket", unix))]
pub use unix::UnixStreamTransport;
#[cfg(feature = "ws")]
pub use websocket::{WebSocketAdapter, WebSocketTransport};

use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use tokio::time::Duration;

/// Error types that can occur during transport operations.
///
/// This enum covers all possible errors that may happen during transport layer operations
/// including I/O errors, TLS errors, WebSocket errors, timeouts, and connection failures.
#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    #[cfg(feature = "tls")]
    Tls(Box<dyn std::error::Error + Send + Sync>),
    #[cfg(feature = "ws")]
    WebSocket(Box<dyn std::error::Error + Send + Sync>),
    #[cfg(feature = "quic")]
    Quic(Box<dyn std::error::Error + Send + Sync>),
    Timeout,
    Connect(String),
    NotConnected,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "IO error: {e}"),
            #[cfg(feature = "tls")]
            TransportError::Tls(e) => write!(f, "TLS error: {e}"),
            #[cfg(feature = "ws")]
            TransportError::WebSocket(e) => write!(f, "WebSocket error: {e}"),
            #[cfg(feature = "quic")]
            TransportError::Quic(e) => write!(f, "QUIC error: {e}"),
            TransportError::Timeout => write!(f, "Operation timed out"),
            TransportError::Connect(msg) => write!(f, "Connection failed: {msg}"),
            TransportError::NotConnected => write!(f, "Transport not connected"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io(e)
    }
}

/// Core trait that defines the transport layer operations for MQTT connections.
///
/// This trait must be implemented by all transport implementations. It provides
/// asynchronous methods for data transmission, data reception, and connection shutdown.
///
/// # Custom Transport Implementation
///
/// Users can implement their own custom transport by implementing this trait.
/// The implementation must handle the underlying network communication and provide
/// the required async operations.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{TransportOps, TransportError};
/// use mqtt_endpoint_tokio::mqtt_ep::{Endpoint, Version, role, Mode};
/// use std::io::IoSlice;
/// use std::pin::Pin;
/// use std::future::Future;
/// use tokio::time::Duration;
///
/// struct MyCustomTransport {
///     // Your transport-specific fields
/// }
///
/// impl TransportOps for MyCustomTransport {
///     fn send<'a>(
///         &'a mut self,
///         buffers: &'a [IoSlice<'a>],
///     ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
///         Box::pin(async move {
///             // Implement your send logic
///             Ok(())
///         })
///     }
///
///     fn recv<'a>(
///         &'a mut self,
///         buffer: &'a mut [u8],
///     ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
///         Box::pin(async move {
///             // Implement your receive logic
///             Ok(0)
///         })
///     }
///
///     fn shutdown<'a>(
///         &'a mut self,
///         timeout: Duration,
///     ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
///         Box::pin(async move {
///             // Implement your shutdown logic
///         })
///     }
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create custom transport and use it with Endpoint
/// let custom_transport: Box<dyn TransportOps + Send> = Box::new(MyCustomTransport {});
/// let mut endpoint = Endpoint::<role::Client>::new(Version::V5_0);
/// endpoint.attach(custom_transport, Mode::Client).await?;
/// # Ok(())
/// # }
/// ```
pub trait TransportOps {
    /// Sends data through the transport layer.
    ///
    /// This method sends the provided data buffers through the underlying transport.
    /// The implementation should handle vectored I/O efficiently when possible.
    ///
    /// # Parameters
    ///
    /// * `buffers` - Array of I/O slices containing the data to send
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all data is successfully sent,
    /// or a [`TransportError`] if sending fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TransportOps, TcpTransport};
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::connect_helper;
    /// use std::io::IoSlice;
    /// use std::net::SocketAddr;
    ///
    /// let addr: SocketAddr = "127.0.0.1:1883".parse()?;
    /// // Use connect_helper to establish connection first
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let mut transport = TcpTransport::from_stream(tcp_stream);
    ///
    /// let data = b"Hello, MQTT!";
    /// let buffers = [IoSlice::new(data)];
    /// transport.send(&buffers).await?;
    /// # Ok(())
    /// # }
    /// ```
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;

    /// Receives data from the transport layer.
    ///
    /// This method reads data from the underlying transport into the provided buffer.
    /// The method may return before the buffer is completely filled.
    ///
    /// # Parameters
    ///
    /// * `buffer` - Mutable buffer to store the received data
    ///
    /// # Returns
    ///
    /// Returns `Ok(bytes_read)` containing the number of bytes actually read,
    /// or a [`TransportError`] if receiving fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TransportOps, TcpTransport, connect_helper};
    /// use std::net::SocketAddr;
    ///
    /// let addr: SocketAddr = "127.0.0.1:1883".parse()?;
    /// // Use connect_helper to establish connection first
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let mut transport = TcpTransport::from_stream(tcp_stream);
    ///
    /// let mut buffer = [0u8; 1024];
    /// let bytes_read = transport.recv(&mut buffer).await?;
    /// println!("Received {bytes_read} bytes");
    /// # Ok(())
    /// # }
    /// ```
    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>>;

    /// Gracefully shuts down the transport connection.
    ///
    /// This method attempts to close the underlying transport connection gracefully
    /// within the specified timeout duration. If the timeout expires, the connection
    /// may be forcibly closed.
    ///
    /// # Parameters
    ///
    /// * `timeout` - Maximum duration to wait for graceful shutdown
    ///
    /// # Examples
    ///
    /// ```rust
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TransportOps, TcpTransport};
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::connect_helper;
    /// use tokio::time::Duration;
    /// use std::net::SocketAddr;
    ///
    /// let addr: SocketAddr = "127.0.0.1:1883".parse()?;
    /// // Use connect_helper to establish connection first
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let mut transport = TcpTransport::from_stream(tcp_stream);
    ///
    /// // Shutdown with 5 second timeout
    /// transport.shutdown(Duration::from_secs(5)).await;
    /// # Ok(())
    /// # }
    /// ```
    fn shutdown<'a>(
        &'a mut self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// Implementation of [`TransportOps`] for boxed trait objects.
///
/// This allows using transport implementations through trait objects,
/// enabling dynamic dispatch for different transport types at runtime.
///
/// # Examples
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{TransportOps, TcpTransport, connect_helper};
/// use std::net::SocketAddr;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let addr: SocketAddr = "127.0.0.1:1883".parse().unwrap();
/// // Use connect_helper to establish connection first
/// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
/// let transport: Box<dyn TransportOps + Send> = Box::new(TcpTransport::from_stream(tcp_stream));
/// // Can now use transport through the trait object
/// # Ok(())
/// # }
/// ```
impl TransportOps for Box<dyn TransportOps + Send> {
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        (**self).send(buffers)
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        (**self).recv(buffer)
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        (**self).shutdown(timeout)
    }
}
