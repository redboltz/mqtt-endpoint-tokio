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
use std::io::IoSlice;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

#[derive(Debug)]
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    pub async fn connect(addr: &str) -> Result<Self, TransportError> {
        Self::connect_with_config(addr, &ClientConfig::default()).await
    }

    pub async fn connect_with_config(
        addr: &str,
        config: &ClientConfig,
    ) -> Result<Self, TransportError> {
        let stream = timeout(config.connect_timeout, TcpStream::connect(addr))
            .await
            .map_err(|_| TransportError::Timeout)?
            .map_err(TransportError::Io)?;

        Ok(Self::from_stream(stream))
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
    async fn send(&mut self, buffers: &[IoSlice<'_>]) -> Result<(), TransportError> {
        for buf in buffers {
            self.stream
                .write_all(buf)
                .await
                .map_err(TransportError::Io)?;
        }
        Ok(())
    }

    async fn recv(&mut self, buffer: &mut [u8]) -> Result<usize, TransportError> {
        self.stream.read(buffer).await.map_err(TransportError::Io)
    }

    async fn shutdown(&mut self, timeout_duration: Duration) {
        // Try graceful shutdown first with timeout
        let graceful_result = timeout(timeout_duration, self.stream.shutdown()).await;
        
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
}
