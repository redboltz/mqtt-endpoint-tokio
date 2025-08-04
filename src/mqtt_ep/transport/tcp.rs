/**
 * MIT License
 *
 * Copyright (c) 2025 Takatoshi Kondo
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use super::{TransportError, TransportOps};
use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// TCP transport implementation for MQTT connections.
///
/// This transport provides basic TCP socket connectivity for MQTT communication.
/// It accepts already established TCP streams using [`TcpTransport::from_stream`].
/// For connection establishment, use the helper functions in [`crate::mqtt_ep::transport::connect_helper`].
///
/// # Examples
///
/// ## Using with connect helper
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{TcpTransport, connect_helper};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
/// let transport = TcpTransport::from_stream(tcp_stream);
/// # Ok(())
/// # }
/// ```
///
/// ## Server-side Usage
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::TcpTransport;
/// use tokio::net::TcpListener;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let listener = TcpListener::bind("127.0.0.1:1883").await?;
/// let (stream, _) = listener.accept().await?;
/// let transport = TcpTransport::from_stream(stream);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Creates a TCP transport from an already established TCP stream.
    ///
    /// This is typically used on the server side when accepting incoming connections.
    /// The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `stream` - An already established TCP stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::TcpTransport;
    /// use tokio::net::TcpListener;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = TcpListener::bind("127.0.0.1:1883").await?;
    /// let (stream, _) = listener.accept().await?;
    /// let transport = TcpTransport::from_stream(stream);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }
}

impl TransportOps for TcpTransport {
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

            // Flush the TCP stream to ensure data is actually sent
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
            // Try graceful shutdown first with timeout
            let graceful_result = timeout(timeout_duration, self.stream.shutdown()).await;

            // If graceful shutdown fails or times out, the connection will be closed
            // when the stream goes out of scope
            match graceful_result {
                Ok(Ok(())) => {
                    // Graceful shutdown succeeded
                }
                Ok(Err(_io_error)) => {
                    // Graceful shutdown failed, force close by dropping the stream
                }
                Err(_timeout_error) => {
                    // Timeout occurred, force close by dropping the stream
                }
            }
        })
    }
}

impl TcpTransport {
    /// Provides mutable access to the underlying TCP stream for custom configuration.
    ///
    /// This method allows users to directly configure the underlying `TcpStream`
    /// for options not covered by the transport abstraction, such as:
    /// - Socket buffer sizes (platform-dependent)
    /// - TCP_NODELAY (`set_nodelay`)
    /// - Keep-alive settings (`set_keepalive`)
    /// - Time-to-live (`set_ttl`)
    ///
    /// # Safety and Responsibility
    ///
    /// Users are responsible for ensuring that any configuration changes do not
    /// interfere with the transport's normal operation. Modifying certain stream
    /// properties may affect the reliability of MQTT communication.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TcpTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let mut transport = TcpTransport::from_stream(tcp_stream);
    ///
    /// // Configure the underlying TCP stream
    /// let stream = transport.stream_mut();
    /// stream.set_nodelay(true)?;
    /// // Note: recv/send buffer size configuration depends on platform
    /// // and may require socket2 crate for cross-platform support
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Provides immutable access to the underlying TCP stream for inspection.
    ///
    /// This method allows users to inspect the current state and configuration
    /// of the underlying `TcpStream` without modifying it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{TcpTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let transport = TcpTransport::from_stream(tcp_stream);
    ///
    /// // Inspect the underlying TCP stream
    /// let stream = transport.stream();
    /// let peer_addr = stream.peer_addr()?;
    /// let local_addr = stream.local_addr()?;
    /// println!("Connected: {local_addr} -> {peer_addr}");
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }
}
