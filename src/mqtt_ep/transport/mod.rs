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
mod tcp;
mod tls;
mod websocket;

pub use tcp::TcpTransport;
pub use tls::TlsTransport;
pub use websocket::{WebSocketAdapter, WebSocketTransport};

use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use tokio::time::Duration;

#[derive(Debug)]
pub enum Transport {
    Tcp(TcpTransport),
    Tls(TlsTransport),
    WebSocket(WebSocketTransport),
    WebSocketTls(WebSocketTransport),
}

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    Tls(Box<dyn std::error::Error + Send + Sync>),
    WebSocket(Box<dyn std::error::Error + Send + Sync>),
    Timeout,
    Handshake(String),
    NotConnected,
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "IO error: {}", e),
            TransportError::Tls(e) => write!(f, "TLS error: {}", e),
            TransportError::WebSocket(e) => write!(f, "WebSocket error: {}", e),
            TransportError::Timeout => write!(f, "Operation timed out"),
            TransportError::Handshake(msg) => write!(f, "Handshake failed: {}", msg),
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

pub trait TransportOps {
    /// Perform transport layer handshake (connection establishment)
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>>;
    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>>;
    fn shutdown<'a>(
        &'a mut self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

impl TransportOps for Transport {
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Transport::Tcp(t) => t.handshake().await,
                Transport::Tls(t) => t.handshake().await,
                Transport::WebSocket(t) => t.handshake().await,
                Transport::WebSocketTls(t) => t.handshake().await,
            }
        })
    }

    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Transport::Tcp(t) => t.send(buffers).await,
                Transport::Tls(t) => t.send(buffers).await,
                Transport::WebSocket(t) => t.send(buffers).await,
                Transport::WebSocketTls(t) => t.send(buffers).await,
            }
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Transport::Tcp(t) => t.recv(buffer).await,
                Transport::Tls(t) => t.recv(buffer).await,
                Transport::WebSocket(t) => t.recv(buffer).await,
                Transport::WebSocketTls(t) => t.recv(buffer).await,
            }
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Transport::Tcp(t) => t.shutdown(timeout).await,
                Transport::Tls(t) => t.shutdown(timeout).await,
                Transport::WebSocket(t) => t.shutdown(timeout).await,
                Transport::WebSocketTls(t) => t.shutdown(timeout).await,
            }
        })
    }
}

// Implement TransportOps for Box<dyn TransportOps + Send> to allow trait object usage
impl TransportOps for Box<dyn TransportOps + Send> {
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        (**self).handshake()
    }

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

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub connect_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

pub struct ServerConfig {
    pub accept_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            accept_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}
