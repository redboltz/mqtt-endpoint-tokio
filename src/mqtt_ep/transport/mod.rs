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
pub use websocket::{WebSocketTransport, WebSocketAdapter};

use std::io::IoSlice;
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
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "IO error: {}", e),
            TransportError::Tls(e) => write!(f, "TLS error: {}", e),
            TransportError::WebSocket(e) => write!(f, "WebSocket error: {}", e),
            TransportError::Timeout => write!(f, "Operation timed out"),
            TransportError::Handshake(msg) => write!(f, "Handshake failed: {}", msg),
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
    async fn send(&mut self, buffers: &[IoSlice<'_>]) -> Result<(), TransportError>;
    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError>;
    async fn shutdown(&mut self, timeout: Duration) -> Result<(), TransportError>;
}

impl TransportOps for Transport {
    async fn send(&mut self, buffers: &[IoSlice<'_>]) -> Result<(), TransportError> {
        match self {
            Transport::Tcp(t) => t.send(buffers).await,
            Transport::Tls(t) => t.send(buffers).await,
            Transport::WebSocket(t) => t.send(buffers).await,
            Transport::WebSocketTls(t) => t.send(buffers).await,
        }
    }

    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        match self {
            Transport::Tcp(t) => t.recv(buffer).await,
            Transport::Tls(t) => t.recv(buffer).await,
            Transport::WebSocket(t) => t.recv(buffer).await,
            Transport::WebSocketTls(t) => t.recv(buffer).await,
        }
    }

    async fn shutdown(&mut self, timeout: Duration) -> Result<(), TransportError> {
        match self {
            Transport::Tcp(t) => t.shutdown(timeout).await,
            Transport::Tls(t) => t.shutdown(timeout).await,
            Transport::WebSocket(t) => t.shutdown(timeout).await,
            Transport::WebSocketTls(t) => t.shutdown(timeout).await,
        }
    }
}

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