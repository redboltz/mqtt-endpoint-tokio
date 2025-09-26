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
use quinn::{RecvStream, SendStream};
use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use tokio::time::{timeout, Duration};

/// QUIC transport implementation for MQTT connections.
///
/// This transport provides QUIC connectivity for MQTT communication using the Quinn library.
/// It accepts already established QUIC streams using [`QuicTransport::from_streams`].
/// For connection establishment, use the helper functions in [`crate::mqtt_ep::transport::connect_helper`].
///
/// # Examples
///
/// ## Using with connect helper
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{QuicTransport, connect_helper};
/// use tokio::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let (send_stream, recv_stream) = connect_helper::connect_quic("127.0.0.1:4433", None, Some(Duration::from_secs(5))).await?;
/// let transport = QuicTransport::from_streams(send_stream, recv_stream);
/// # Ok(())
/// # }
/// ```
///
/// ## Server-side Usage
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::QuicTransport;
/// use quinn::{Endpoint, ServerConfig};
/// use rustls::pki_types::{CertificateDer, PrivateKeyDer};
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # // This is a simplified example - real certificates are needed
/// # let cert_der = CertificateDer::from(vec![]);
/// # let private_key = PrivateKeyDer::try_from(vec![])?;
/// # let rustls_server_config = Arc::new(
/// #     rustls::ServerConfig::builder()
/// #         .with_no_client_auth()
/// #         .with_single_cert(vec![cert_der], private_key)?
/// # );
/// # let server_config = ServerConfig::with_crypto(Arc::new(
/// #     quinn::crypto::rustls::QuicServerConfig::try_from(rustls_server_config)?
/// # ));
/// let endpoint = Endpoint::server(server_config, "127.0.0.1:4433".parse()?)?;
///
/// if let Some(conn) = endpoint.accept().await {
///     let connection = conn.await?;
///     let (send_stream, recv_stream) = connection.accept_bi().await?;
///     let transport = QuicTransport::from_streams(send_stream, recv_stream);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct QuicTransport {
    send_stream: SendStream,
    recv_stream: RecvStream,
}

impl QuicTransport {
    /// Creates a QUIC transport from already established QUIC streams.
    ///
    /// This is used when you have separate send and receive streams from a QUIC connection.
    /// The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `send_stream` - QUIC send stream for writing data
    /// * `recv_stream` - QUIC receive stream for reading data
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::QuicTransport;
    /// use quinn::{Endpoint, ClientConfig};
    /// use rustls::pki_types::CertificateDer;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # // Create a simple client config that accepts any certificate (for testing)
    /// # let mut root_cert_store = rustls::RootCertStore::empty();
    /// # let rustls_client_config = Arc::new(
    /// #     rustls::ClientConfig::builder()
    /// #         .with_root_certificates(root_cert_store)
    /// #         .with_no_client_auth()
    /// # );
    /// # let client_config = ClientConfig::new(Arc::new(
    /// #     quinn::crypto::rustls::QuicClientConfig::try_from(rustls_client_config)?
    /// # ));
    /// let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    /// endpoint.set_default_client_config(client_config);
    ///
    /// let connection = endpoint.connect("127.0.0.1:4433".parse()?, "localhost")?.await?;
    /// let (send_stream, recv_stream) = connection.open_bi().await?;
    /// let transport = QuicTransport::from_streams(send_stream, recv_stream);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_streams(send_stream: SendStream, recv_stream: RecvStream) -> Self {
        Self {
            send_stream,
            recv_stream,
        }
    }
}

impl TransportOps for QuicTransport {
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            use tokio::io::AsyncWriteExt;

            // Calculate total bytes to send
            let total_bytes: usize = buffers.iter().map(|buf| buf.len()).sum();

            // QUIC streams support vectored writes, but we need to handle partial writes
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
                    .send_stream
                    .write_vectored(&current_buffers)
                    .await
                    .map_err(|e| TransportError::Quic(Box::new(e)))?;

                total_written += bytes_written;

                if bytes_written == 0 {
                    return Err(TransportError::Quic(Box::new(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "write_vectored returned 0 bytes written",
                    ))));
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

            // Flush the QUIC stream to ensure data is sent
            self.send_stream
                .flush()
                .await
                .map_err(|e| TransportError::Quic(Box::new(e)))?;

            Ok(())
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self.recv_stream.read(buffer).await {
                Ok(Some(size)) => Ok(size),
                Ok(None) => Ok(0), // Stream closed
                Err(e) => Err(TransportError::Quic(Box::new(e))),
            }
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout_duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Try graceful shutdown with timeout
            let graceful_result = timeout(timeout_duration, async {
                // Finish the send stream gracefully
                let _ = self.send_stream.finish();
                // Close the receive stream
                let _ = self.recv_stream.stop(0u32.into());
            })
            .await;

            // If timeout occurs, streams will be dropped which forces closure
            match graceful_result {
                Ok(()) => {
                    // Graceful shutdown succeeded
                }
                Err(_timeout_error) => {
                    // Timeout occurred, force close by dropping streams
                }
            }
        })
    }
}

impl QuicTransport {
    /// Provides mutable access to the underlying QUIC send stream for custom configuration.
    ///
    /// This method allows users to directly configure the underlying `SendStream`
    /// for options not covered by the transport abstraction.
    ///
    /// # Safety and Responsibility
    ///
    /// Users are responsible for ensuring that any configuration changes do not
    /// interfere with the transport's normal operation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{QuicTransport, connect_helper};
    /// use tokio::time::Duration;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (send_stream, recv_stream) = connect_helper::connect_quic("127.0.0.1:4433", None, Some(Duration::from_secs(5))).await?;
    /// let mut transport = QuicTransport::from_streams(send_stream, recv_stream);
    ///
    /// // Configure the underlying QUIC send stream
    /// let send_stream = transport.send_stream_mut();
    /// // Custom stream configuration can be done here
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_stream_mut(&mut self) -> &mut SendStream {
        &mut self.send_stream
    }

    /// Provides immutable access to the underlying QUIC send stream for inspection.
    ///
    /// This method allows users to inspect the current state of the underlying `SendStream`.
    pub fn send_stream(&self) -> &SendStream {
        &self.send_stream
    }

    /// Provides mutable access to the underlying QUIC receive stream for custom configuration.
    ///
    /// This method allows users to directly configure the underlying `RecvStream`
    /// for options not covered by the transport abstraction.
    ///
    /// # Safety and Responsibility
    ///
    /// Users are responsible for ensuring that any configuration changes do not
    /// interfere with the transport's normal operation.
    pub fn recv_stream_mut(&mut self) -> &mut RecvStream {
        &mut self.recv_stream
    }

    /// Provides immutable access to the underlying QUIC receive stream for inspection.
    ///
    /// This method allows users to inspect the current state of the underlying `RecvStream`.
    pub fn recv_stream(&self) -> &RecvStream {
        &self.recv_stream
    }
}
