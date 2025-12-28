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
use tokio::net::UnixStream;
use tokio::time::{timeout, Duration};

/// Unix domain socket transport implementation for MQTT connections.
///
/// This transport provides Unix domain socket connectivity for MQTT communication
/// on Unix-based systems. It's ideal for local inter-process communication where
/// the client and broker run on the same machine, offering better performance
/// than TCP/IP sockets.
///
/// # Platform Support
///
/// This transport is only available on Unix-based systems (Linux, macOS, BSD, etc.).
///
/// # Examples
///
/// ## Using with connect helper
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::{UnixStreamTransport, connect_helper};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let unix_stream = connect_helper::connect_unix("/tmp/mqtt.sock", None).await?;
/// let transport = UnixStreamTransport::from_stream(unix_stream);
/// # Ok(())
/// # }
/// ```
///
/// ## Server-side Usage
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::UnixStreamTransport;
/// use tokio::net::UnixListener;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let listener = UnixListener::bind("/tmp/mqtt.sock")?;
/// let (stream, _) = listener.accept().await?;
/// let transport = UnixStreamTransport::from_stream(stream);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct UnixStreamTransport {
    stream: UnixStream,
}

impl UnixStreamTransport {
    /// Creates a Unix domain socket transport from an already established Unix stream.
    ///
    /// This is typically used on the server side when accepting incoming connections.
    /// The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `stream` - An already established Unix stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::UnixStreamTransport;
    /// use tokio::net::UnixListener;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = UnixListener::bind("/tmp/mqtt.sock")?;
    /// let (stream, _) = listener.accept().await?;
    /// let transport = UnixStreamTransport::from_stream(stream);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }
}

impl TransportOps for UnixStreamTransport {
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

            // Flush the Unix stream to ensure data is actually sent
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

impl UnixStreamTransport {
    /// Provides mutable access to the underlying Unix stream for custom configuration.
    ///
    /// This method allows users to directly configure the underlying `UnixStream`
    /// for options not covered by the transport abstraction.
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
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{UnixStreamTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let unix_stream = connect_helper::connect_unix("/tmp/mqtt.sock", None).await?;
    /// let mut transport = UnixStreamTransport::from_stream(unix_stream);
    ///
    /// // Access the underlying Unix stream
    /// let stream = transport.stream_mut();
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream_mut(&mut self) -> &mut UnixStream {
        &mut self.stream
    }

    /// Provides immutable access to the underlying Unix stream for inspection.
    ///
    /// This method allows users to inspect the current state and configuration
    /// of the underlying `UnixStream` without modifying it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::{UnixStreamTransport, connect_helper};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let unix_stream = connect_helper::connect_unix("/tmp/mqtt.sock", None).await?;
    /// let transport = UnixStreamTransport::from_stream(unix_stream);
    ///
    /// // Inspect the underlying Unix stream
    /// let stream = transport.stream();
    /// let peer_cred = stream.peer_cred()?;
    /// println!("Connected peer PID: {}", peer_cred.pid().unwrap_or(0));
    /// # Ok(())
    /// # }
    /// ```
    pub fn stream(&self) -> &UnixStream {
        &self.stream
    }
}
