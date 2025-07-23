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
use std::future::Future;
use std::io::IoSlice;
use std::net::SocketAddr;
use std::pin::Pin;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

#[derive(Debug)]
pub struct TcpTransport {
    stream: Option<TcpStream>,
    remote_addr: Option<SocketAddr>,
    config: ClientConfig,
}

impl TcpTransport {
    /// Create a new TcpTransport for the given remote address (not yet connected)
    pub fn new(remote_addr: SocketAddr) -> Self {
        Self {
            stream: None,
            remote_addr: Some(remote_addr),
            config: ClientConfig::default(),
        }
    }

    /// Create a new TcpTransport for the given remote address with custom config
    pub fn new_with_config(remote_addr: SocketAddr, config: ClientConfig) -> Self {
        Self {
            stream: None,
            remote_addr: Some(remote_addr),
            config,
        }
    }

    /// Create TcpTransport from an already established stream
    pub fn from_stream(stream: TcpStream) -> Self {
        Self {
            stream: Some(stream),
            remote_addr: None,
            config: ClientConfig::default(),
        }
    }

    /// Legacy method for backward compatibility
    pub async fn connect(addr: &str) -> Result<Self, TransportError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| TransportError::Handshake(format!("Invalid address: {}", e)))?;
        let mut transport = Self::new(addr);
        transport.handshake().await?;
        Ok(transport)
    }

    /// Legacy method for backward compatibility
    pub async fn connect_with_config(
        addr: &str,
        config: &ClientConfig,
    ) -> Result<Self, TransportError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| TransportError::Handshake(format!("Invalid address: {}", e)))?;
        let mut transport = Self::new_with_config(addr, config.clone());
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
        let (stream, _addr) = timeout(config.accept_timeout, listener.accept())
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        Ok(Self::from_stream(stream))
    }
}

impl TransportOps for TcpTransport {
    fn handshake<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            // If already connected, nothing to do
            if self.stream.is_some() {
                return Ok(());
            }

            // Perform TCP connection
            let remote_addr = self.remote_addr.ok_or_else(|| {
                TransportError::Handshake("No remote address specified".to_string())
            })?;

            let stream = timeout(self.config.connect_timeout, TcpStream::connect(remote_addr))
                .await
                .map_err(|_| TransportError::Timeout)?
                .map_err(TransportError::Io)?;

            self.stream = Some(stream);
            Ok(())
        })
    }

    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.stream.as_mut().ok_or(TransportError::NotConnected)?;

            stream
                .write_vectored(buffers)
                .await
                .map(|_| ()) // Ignore bytes written count
                .map_err(TransportError::Io)
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let stream = self.stream.as_mut().ok_or(TransportError::NotConnected)?;
            stream.read(buffer).await.map_err(TransportError::Io)
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout_duration: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Some(ref mut stream) = self.stream {
                // Try graceful shutdown first with timeout
                let graceful_result = timeout(timeout_duration, stream.shutdown()).await;

                // If graceful shutdown fails or times out, force close the connection
                match graceful_result {
                    Ok(Ok(())) => {
                        // Graceful shutdown succeeded
                    }
                    Ok(Err(_io_error)) => {
                        // Graceful shutdown failed, force close by dropping the stream
                        // The stream will be closed when it goes out of scope
                    }
                    Err(_timeout_error) => {
                        // Timeout occurred, force close by dropping the stream
                        // The stream will be closed when it goes out of scope
                    }
                }
            }

            // Clear the stream to indicate disconnected state
            self.stream = None;
        })
    }
}
