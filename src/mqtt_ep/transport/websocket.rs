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
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    tungstenite::{Error as WsError, Message},
    WebSocketStream,
};

/// WebSocket transport implementation for MQTT-over-WebSocket connections.
///
/// This transport provides WebSocket connectivity for MQTT communication,
/// supporting both plain WebSocket and WebSocket over TLS connections.
/// It's commonly used for web-based MQTT clients.
///
/// # Examples
///
/// ## TCP Client Stream
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
/// use tokio_tungstenite::WebSocketStream;
/// use tokio::net::TcpStream;
///
/// # async fn example(ws_stream: WebSocketStream<TcpStream>) {
/// let transport = WebSocketTransport::from_tcp_client_stream(ws_stream);
/// # }
/// ```
///
/// ## TLS Client Stream
///
/// ```rust
/// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
/// use tokio_tungstenite::WebSocketStream;
/// use tokio_rustls::client::TlsStream;
/// use tokio::net::TcpStream;
///
/// # async fn example(ws_stream: WebSocketStream<TlsStream<TcpStream>>) {
/// let transport = WebSocketTransport::from_tls_client_stream(ws_stream);
/// # }
/// ```
#[derive(Debug)]
pub enum WebSocketTransport {
    TcpClient(WebSocketAdapter<TcpStream>),
    TlsClient(WebSocketAdapter<tokio_rustls::client::TlsStream<TcpStream>>),
    TcpServer(WebSocketAdapter<TcpStream>),
    TlsServer(WebSocketAdapter<tokio_rustls::server::TlsStream<TcpStream>>),
}

/// Adapter that wraps a WebSocket stream for MQTT protocol communication.
///
/// This adapter handles the conversion between MQTT binary data and WebSocket
/// message frames, providing buffering for efficient data handling.
///
/// The generic parameter `S` represents the underlying stream type (TCP or TLS).
#[derive(Debug)]
pub struct WebSocketAdapter<S> {
    ws: WebSocketStream<S>,
    read_buffer: Vec<u8>,
    read_pos: usize,
}

impl WebSocketTransport {
    /// Creates a WebSocket transport from a TCP client WebSocket stream.
    ///
    /// This is typically used on the client side when connecting via plain WebSocket
    /// connections over TCP. The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `ws` - An already established TCP WebSocket stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
    /// use tokio_tungstenite::WebSocketStream;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(ws_stream: WebSocketStream<TcpStream>) {
    /// let transport = WebSocketTransport::from_tcp_client_stream(ws_stream);
    /// # }
    /// ```
    pub fn from_tcp_client_stream(ws: WebSocketStream<TcpStream>) -> Self {
        Self::TcpClient(WebSocketAdapter::new(ws))
    }

    /// Creates a WebSocket transport from a TLS client WebSocket stream.
    ///
    /// This is typically used on the client side when connecting via secure WebSocket
    /// connections over TLS. The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `ws` - An already established client-side TLS WebSocket stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
    /// use tokio_tungstenite::WebSocketStream;
    /// use tokio_rustls::client::TlsStream;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(ws_stream: WebSocketStream<TlsStream<TcpStream>>) {
    /// let transport = WebSocketTransport::from_tls_client_stream(ws_stream);
    /// # }
    /// ```
    pub fn from_tls_client_stream(
        ws: WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>,
    ) -> Self {
        Self::TlsClient(WebSocketAdapter::new(ws))
    }

    /// Creates a WebSocket transport from a TCP server WebSocket stream.
    ///
    /// This is typically used on the server side when accepting incoming plain WebSocket
    /// connections over TCP. The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `ws` - An already established TCP WebSocket stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
    /// use tokio_tungstenite::WebSocketStream;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(ws_stream: WebSocketStream<TcpStream>) {
    /// let transport = WebSocketTransport::from_tcp_server_stream(ws_stream);
    /// # }
    /// ```
    pub fn from_tcp_server_stream(ws: WebSocketStream<TcpStream>) -> Self {
        Self::TcpServer(WebSocketAdapter::new(ws))
    }

    /// Creates a WebSocket transport from a TLS server WebSocket stream.
    ///
    /// This is typically used on the server side when accepting incoming secure WebSocket
    /// connections over TLS. The transport is created in a connected state.
    ///
    /// # Parameters
    ///
    /// * `ws` - An already established server-side TLS WebSocket stream
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketTransport;
    /// use tokio_tungstenite::WebSocketStream;
    /// use tokio_rustls::server::TlsStream;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(ws_stream: WebSocketStream<TlsStream<TcpStream>>) {
    /// let transport = WebSocketTransport::from_tls_server_stream(ws_stream);
    /// # }
    /// ```
    pub fn from_tls_server_stream(
        ws: WebSocketStream<tokio_rustls::server::TlsStream<TcpStream>>,
    ) -> Self {
        Self::TlsServer(WebSocketAdapter::new(ws))
    }
}

impl<S> WebSocketAdapter<S> {
    /// Creates a new WebSocket adapter wrapping the provided WebSocket stream.
    ///
    /// This method initializes the internal buffer used for data handling
    /// and prepares the adapter for MQTT communication over WebSocket.
    ///
    /// # Parameters
    ///
    /// * `ws` - The WebSocket stream to wrap
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::WebSocketAdapter;
    /// use tokio_tungstenite::{WebSocketStream, MaybeTlsStream};
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example(ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>) {
    /// let adapter = WebSocketAdapter::new(ws_stream);
    /// # }
    /// ```
    pub fn new(ws: WebSocketStream<S>) -> Self {
        Self {
            ws,
            read_buffer: Vec::new(),
            read_pos: 0,
        }
    }

    async fn ensure_data(&mut self) -> Result<(), TransportError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        if self.read_pos >= self.read_buffer.len() {
            use futures_util::StreamExt;

            match self.ws.next().await {
                Some(Ok(Message::Binary(data))) => {
                    self.read_buffer = data;
                    self.read_pos = 0;
                    Ok(())
                }
                Some(Ok(Message::Close(_))) => Err(TransportError::WebSocket(Box::new(
                    WsError::ConnectionClosed,
                ))),
                Some(Ok(_)) => {
                    // Text messages are not expected for MQTT
                    Err(TransportError::WebSocket(Box::new(
                        WsError::ConnectionClosed,
                    )))
                }
                Some(Err(e)) => Err(TransportError::WebSocket(Box::new(e))),
                None => Err(TransportError::WebSocket(Box::new(
                    WsError::ConnectionClosed,
                ))),
            }
        } else {
            Ok(())
        }
    }
}

impl TransportOps for WebSocketTransport {
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            // Calculate total size and pre-allocate
            let total_len: usize = buffers.iter().map(|buf| buf.len()).sum();
            let mut combined = Vec::with_capacity(total_len);

            for buf in buffers {
                combined.extend_from_slice(buf);
            }

            let message = Message::Binary(combined);

            match self {
                WebSocketTransport::TcpClient(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
                WebSocketTransport::TlsClient(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
                WebSocketTransport::TcpServer(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
                WebSocketTransport::TlsServer(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
            }
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                WebSocketTransport::TcpClient(adapter) => {
                    adapter.ensure_data().await?;

                    let available = adapter.read_buffer.len() - adapter.read_pos;
                    let to_copy = buffer.len().min(available);

                    if to_copy > 0 {
                        buffer[..to_copy].copy_from_slice(
                            &adapter.read_buffer[adapter.read_pos..adapter.read_pos + to_copy],
                        );
                        adapter.read_pos += to_copy;
                    }

                    Ok(to_copy)
                }
                WebSocketTransport::TlsClient(adapter) => {
                    adapter.ensure_data().await?;

                    let available = adapter.read_buffer.len() - adapter.read_pos;
                    let to_copy = buffer.len().min(available);

                    if to_copy > 0 {
                        buffer[..to_copy].copy_from_slice(
                            &adapter.read_buffer[adapter.read_pos..adapter.read_pos + to_copy],
                        );
                        adapter.read_pos += to_copy;
                    }

                    Ok(to_copy)
                }
                WebSocketTransport::TcpServer(adapter) => {
                    adapter.ensure_data().await?;

                    let available = adapter.read_buffer.len() - adapter.read_pos;
                    let to_copy = buffer.len().min(available);

                    if to_copy > 0 {
                        buffer[..to_copy].copy_from_slice(
                            &adapter.read_buffer[adapter.read_pos..adapter.read_pos + to_copy],
                        );
                        adapter.read_pos += to_copy;
                    }

                    Ok(to_copy)
                }
                WebSocketTransport::TlsServer(adapter) => {
                    adapter.ensure_data().await?;

                    let available = adapter.read_buffer.len() - adapter.read_pos;
                    let to_copy = buffer.len().min(available);

                    if to_copy > 0 {
                        buffer[..to_copy].copy_from_slice(
                            &adapter.read_buffer[adapter.read_pos..adapter.read_pos + to_copy],
                        );
                        adapter.read_pos += to_copy;
                    }

                    Ok(to_copy)
                }
            }
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout_duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use futures_util::SinkExt;

            match self {
                WebSocketTransport::TcpClient(adapter) => {
                    // Try graceful WebSocket shutdown first with timeout
                    let graceful_result = timeout(timeout_duration, async {
                        // Send close frame and wait for acknowledgment
                        adapter.ws.send(Message::Close(None)).await?;
                        adapter.ws.close(None).await?;
                        Ok::<(), WsError>(())
                    })
                    .await;

                    // Handle result (success or failure)
                    match graceful_result {
                        Ok(Ok(())) => {
                            // Graceful WebSocket shutdown succeeded
                        }
                        Ok(Err(_ws_error)) => {
                            // Graceful WebSocket shutdown failed, force close
                        }
                        Err(_timeout_error) => {
                            // Timeout occurred, force close
                        }
                    }
                }
                WebSocketTransport::TlsClient(adapter) => {
                    // Try graceful WebSocket shutdown first with timeout
                    let graceful_result = timeout(timeout_duration, async {
                        // Send close frame and wait for acknowledgment
                        adapter.ws.send(Message::Close(None)).await?;
                        adapter.ws.close(None).await?;
                        Ok::<(), WsError>(())
                    })
                    .await;

                    // Handle result (success or failure)
                    match graceful_result {
                        Ok(Ok(())) => {
                            // Graceful WebSocket shutdown succeeded
                        }
                        Ok(Err(_ws_error)) => {
                            // Graceful WebSocket shutdown failed, force close
                        }
                        Err(_timeout_error) => {
                            // Timeout occurred, force close
                        }
                    }
                }
                WebSocketTransport::TcpServer(adapter) => {
                    // Try graceful WebSocket shutdown first with timeout
                    let graceful_result = timeout(timeout_duration, async {
                        // Send close frame and wait for acknowledgment
                        adapter.ws.send(Message::Close(None)).await?;
                        adapter.ws.close(None).await?;
                        Ok::<(), WsError>(())
                    })
                    .await;

                    // Handle result (success or failure)
                    match graceful_result {
                        Ok(Ok(())) => {
                            // Graceful WebSocket shutdown succeeded
                        }
                        Ok(Err(_ws_error)) => {
                            // Graceful WebSocket shutdown failed, force close
                        }
                        Err(_timeout_error) => {
                            // Timeout occurred, force close
                        }
                    }
                }
                WebSocketTransport::TlsServer(adapter) => {
                    // Try graceful WebSocket shutdown first with timeout
                    let graceful_result = timeout(timeout_duration, async {
                        // Send close frame and wait for acknowledgment
                        adapter.ws.send(Message::Close(None)).await?;
                        adapter.ws.close(None).await?;
                        Ok::<(), WsError>(())
                    })
                    .await;

                    // Handle result (success or failure)
                    match graceful_result {
                        Ok(Ok(())) => {
                            // Graceful WebSocket shutdown succeeded
                        }
                        Ok(Err(_ws_error)) => {
                            // Graceful WebSocket shutdown failed, force close
                        }
                        Err(_timeout_error) => {
                            // Timeout occurred, force close
                        }
                    }
                }
            }
        })
    }
}
