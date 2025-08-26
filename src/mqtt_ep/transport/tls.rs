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

use super::{TransportError, TransportOps};
use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};

/// Trait representing a TLS stream that can be used for async I/O operations.
///
/// This trait is automatically implemented for any type that implements the required
/// async I/O traits. It serves as a type constraint for TLS stream types.
pub trait TlsStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

impl<T> TlsStream for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin {}

/// TLS transport implementation for secure MQTT connections.
///
/// This transport provides TLS-encrypted TCP connectivity for MQTT communication.
/// It accepts already established TLS streams using [`TlsTransport::from_stream`].
/// For connection establishment, use the helper functions in [`crate::mqtt_ep::transport::connect_helper`].
///
/// # Examples
///
/// ## Using with connect helper
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{TlsTransport, connect_helper};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, None).await?;
/// let transport = TlsTransport::from_stream(tls_stream);
/// # Ok(())
/// # }
/// ```
pub struct TlsTransport {
    stream: Box<dyn TlsStream>,
}

impl std::fmt::Debug for TlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsTransport")
            .field("stream", &"<tls stream>")
            .finish()
    }
}

impl TlsTransport {
    /// Creates a TLS transport from an already established TLS stream.
    ///
    /// This is typically used on the server side when accepting incoming TLS connections.
    /// The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `stream` - An already established TLS stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::TlsTransport;
    /// use tokio_rustls::server::TlsStream;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(tls_stream: TlsStream<TcpStream>) {
    /// let transport = TlsTransport::from_stream(tls_stream);
    /// # }
    /// ```
    pub fn from_stream<S>(stream: S) -> Self
    where
        S: TlsStream + 'static,
    {
        Self {
            stream: Box::new(stream),
        }
    }
}

impl TransportOps for TlsTransport {
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            use tokio::io::AsyncWriteExt;

            // Calculate total bytes to send for debugging
            let total_bytes: usize = buffers.iter().map(|buf| buf.len()).sum();

            // Use write_vectored for zero-copy, with proper handling of partial writes
            let mut buffer_start_indices = vec![0usize; buffers.len()];
            let mut total_written = 0usize;

            while total_written < total_bytes {
                // Create IoSlice array for remaining data
                let current_buffers: Vec<std::io::IoSlice> = buffers
                    .iter()
                    .enumerate()
                    .filter_map(|(i, buf)| {
                        let start = buffer_start_indices[i];
                        if start < buf.len() {
                            Some(std::io::IoSlice::new(&buf[start..]))
                        } else {
                            None
                        }
                    })
                    .collect();

                if current_buffers.is_empty() {
                    break;
                }

                let bytes_written = self
                    .stream
                    .write_vectored(&current_buffers)
                    .await
                    .map_err(TransportError::Io)?;

                total_written += bytes_written;

                if bytes_written == 0 {
                    return Err(TransportError::Io(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "write_vectored returned 0 bytes written",
                    )));
                }

                // Update buffer start indices
                let mut remaining_to_skip = bytes_written;
                for (i, buf) in buffers.iter().enumerate() {
                    let available = buf.len() - buffer_start_indices[i];
                    if available > 0 {
                        let to_consume = remaining_to_skip.min(available);
                        buffer_start_indices[i] += to_consume;
                        remaining_to_skip -= to_consume;

                        if remaining_to_skip == 0 {
                            break;
                        }
                    }
                }
            }

            // Flush the TLS stream to ensure data is actually sent
            self.stream.flush().await.map_err(TransportError::Io)?;

            Ok(())
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move { self.stream.read(buffer).await.map_err(TransportError::Io) })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout_duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Try graceful TLS shutdown first with timeout
            let graceful_result = timeout(timeout_duration, self.stream.shutdown()).await;

            // If graceful shutdown fails or times out, force close the connection
            match graceful_result {
                Ok(Ok(())) => {
                    // Graceful TLS shutdown succeeded
                }
                Ok(Err(_io_error)) => {
                    // Graceful TLS shutdown failed, force close by dropping the stream
                    // The stream will be closed when it goes out of scope
                }
                Err(_timeout_error) => {
                    // Timeout occurred, force close by dropping the stream
                    // The stream will be closed when it goes out of scope
                }
            }
        })
    }
}

impl TlsTransport {
    /// Provides mutable access to the underlying TLS stream for custom configuration.
    ///
    /// This method allows users to access the underlying TLS stream, but due to
    /// the trait object nature (`Box<dyn TlsStream>`), direct configuration options
    /// are limited compared to TCP streams.
    ///
    /// # Note
    ///
    /// For TLS-specific configuration (cipher suites, certificates, etc.),
    /// configure the TLS stream before creating the transport.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TlsTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, None).await?;
    /// let mut transport = TlsTransport::from_stream(tls_stream);
    ///
    /// // Access the underlying TLS stream (limited operations available)
    /// let _stream = transport.stream_mut();
    /// // Limited configuration options available for trait objects
    /// // Most TLS configuration should be done before creating the transport
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream_mut(&mut self) -> &mut Box<dyn TlsStream> {
        &mut self.stream
    }

    /// Provides immutable access to the underlying TLS stream for inspection.
    ///
    /// This method allows users to inspect the TLS stream, though options
    /// are limited due to the trait object nature.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TlsTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, None).await?;
    /// let transport = TlsTransport::from_stream(tls_stream);
    ///
    /// // Inspect the underlying TLS stream
    /// let _stream = transport.stream();
    /// // Limited inspection options available for trait objects
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream(&self) -> &dyn TlsStream {
        &*self.stream
    }
}
