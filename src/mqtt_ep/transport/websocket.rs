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
use super::{ClientConfig, ServerConfig, TransportError, TransportOps};
use std::collections::HashMap;
use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, accept_async, connect_async,
    tungstenite::{Error as WsError, Message},
};
use url::Url;

#[derive(Debug)]
pub enum WebSocketTransport {
    Plain(WebSocketAdapter<MaybeTlsStream<TcpStream>>),
    Tls(WebSocketAdapter<tokio_rustls::server::TlsStream<TcpStream>>),
    Unconnected {
        url: String,
        headers: Option<HashMap<String, String>>,
        config: ClientConfig,
    },
}

#[derive(Debug)]
pub struct WebSocketAdapter<S> {
    ws: WebSocketStream<S>,
    read_buffer: Vec<u8>,
    read_pos: usize,
}

impl WebSocketTransport {
    /// Create a new WebSocketTransport for the given URL (not yet connected)
    pub fn new(url: String) -> Self {
        Self::Unconnected {
            url,
            headers: None,
            config: ClientConfig::default(),
        }
    }

    /// Create a new WebSocketTransport with custom config and headers
    pub fn new_with_config(
        url: String,
        config: ClientConfig,
        headers: Option<HashMap<String, String>>,
    ) -> Self {
        Self::Unconnected {
            url,
            headers,
            config,
        }
    }

    /// Create WebSocketTransport from an already established plain WebSocket stream
    pub fn from_stream(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self::Plain(WebSocketAdapter::new(ws))
    }

    /// Create WebSocketTransport from an already established TLS WebSocket stream
    pub fn from_tls_stream(
        ws: WebSocketStream<tokio_rustls::server::TlsStream<TcpStream>>,
    ) -> Self {
        Self::Tls(WebSocketAdapter::new(ws))
    }

    /// Legacy method for backward compatibility
    pub async fn connect(uri: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(uri, &ClientConfig::default()).await
    }

    /// Legacy method for backward compatibility
    pub async fn connect_with_config(
        uri: &str,
        config: &ClientConfig,
    ) -> Result<Self, TransportError> {
        let mut transport = Self::new_with_config(uri.to_string(), config.clone(), None);
        transport.handshake().await?;
        Ok(transport)
    }

    pub async fn accept(listener: &TcpListener) -> Result<Self, TransportError> {
        Self::accept_with_config(listener, &ServerConfig::default()).await
    }

    pub async fn accept_with_config(
        listener: &TcpListener,
        config: &ServerConfig,
    ) -> Result<Self, TransportError> {
        let (tcp_stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let ws_stream = timeout(
            config.accept_timeout,
            accept_async(MaybeTlsStream::Plain(tcp_stream)),
        )
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|e| TransportError::WebSocket(Box::new(e)))?;

        Ok(Self::Plain(WebSocketAdapter::new(ws_stream)))
    }

    pub async fn accept_tls(
        listener: &TcpListener,
        tls_acceptor: Arc<TlsAcceptor>,
    ) -> Result<Self, TransportError> {
        Self::accept_tls_with_config(listener, tls_acceptor, &ServerConfig::default()).await
    }

    pub async fn accept_tls_with_config(
        listener: &TcpListener,
        tls_acceptor: Arc<TlsAcceptor>,
        config: &ServerConfig,
    ) -> Result<Self, TransportError> {
        let (tcp_stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        let tls_stream = timeout(config.accept_timeout, tls_acceptor.accept(tcp_stream))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::Tls(Box::new(e)))?;

        let ws_stream = timeout(config.accept_timeout, accept_async(tls_stream))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(|e| TransportError::WebSocket(Box::new(e)))?;

        Ok(Self::from_tls_stream(ws_stream))
    }
}

impl<S> WebSocketAdapter<S> {
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
                        WsError::ConnectionClosed, // 簡略化
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
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                // Already connected
                WebSocketTransport::Plain(_) | WebSocketTransport::Tls(_) => {
                    return Ok(());
                }
                // Need to establish connection
                WebSocketTransport::Unconnected {
                    url,
                    headers,
                    config,
                } => {
                    // Parse URL
                    let parsed_url = Url::parse(url)
                        .map_err(|e| TransportError::Handshake(format!("Invalid URL: {}", e)))?;

                    // For now, ignore custom headers (simplified implementation)
                    // In a full implementation, you'd need to create a proper Request with headers
                    let _ = headers; // Suppress unused warning

                    // Establish WebSocket connection
                    let (ws_stream, _response) =
                        timeout(config.connect_timeout, connect_async(parsed_url))
                            .await
                            .map_err(|_| TransportError::Timeout)?
                            .map_err(|e| TransportError::WebSocket(Box::new(e)))?;

                    // Update self to connected state
                    *self = Self::Plain(WebSocketAdapter::new(ws_stream));
                    Ok(())
                }
            }
        })
    }

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
                WebSocketTransport::Plain(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
                WebSocketTransport::Tls(adapter) => {
                    use futures_util::SinkExt;
                    adapter
                        .ws
                        .send(message)
                        .await
                        .map_err(|e| TransportError::WebSocket(Box::new(e)))
                }
                WebSocketTransport::Unconnected { .. } => Err(TransportError::NotConnected),
            }
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                WebSocketTransport::Plain(adapter) => {
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
                WebSocketTransport::Tls(adapter) => {
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
                WebSocketTransport::Unconnected { .. } => Err(TransportError::NotConnected),
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
                WebSocketTransport::Plain(adapter) => {
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
                WebSocketTransport::Tls(adapter) => {
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
                WebSocketTransport::Unconnected { .. } => {
                    // Already disconnected, nothing to do
                }
            }

            // Set to unconnected state (we don't have original URL info to restore, so create a dummy one)
            *self = WebSocketTransport::Unconnected {
                url: "ws://disconnected".to_string(),
                headers: None,
                config: ClientConfig::default(),
            };
        })
    }
}
