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
use std::collections::VecDeque;
use std::future::Future;
use std::io::IoSlice;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mqtt_endpoint_tokio::mqtt_ep::TransportError;
use mqtt_endpoint_tokio::mqtt_ep::TransportOps;

/// Call record for tracking method invocations
#[derive(Debug, Clone, PartialEq)]
pub enum TransportCall {
    Send { data: Vec<u8> },
    Recv { buffer_size: usize },
    Shutdown { timeout: Duration },
}

/// Response configuration for controlling stub behavior
#[derive(Debug)]
#[allow(dead_code)]
pub enum TransportResponse {
    SendOk,
    SendErr(TransportError),
    RecvOk(Vec<u8>),
    RecvErr(TransportError),
    Shutdown,
}

impl Clone for TransportResponse {
    fn clone(&self) -> Self {
        match self {
            TransportResponse::SendOk => TransportResponse::SendOk,
            TransportResponse::SendErr(_) => {
                TransportResponse::SendErr(TransportError::NotConnected)
            }
            TransportResponse::RecvOk(data) => TransportResponse::RecvOk(data.clone()),
            TransportResponse::RecvErr(_) => {
                TransportResponse::RecvErr(TransportError::NotConnected)
            }
            TransportResponse::Shutdown => TransportResponse::Shutdown,
        }
    }
}

/// Stub transport implementation for testing
#[derive(Clone)]
pub struct StubTransport {
    /// Record of method calls made to this transport
    pub calls: Arc<Mutex<Vec<TransportCall>>>,
    /// Queue of responses to return for method calls
    responses: Arc<Mutex<VecDeque<TransportResponse>>>,
}

impl StubTransport {
    /// Create a new StubTransport
    pub fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Add a response to the queue
    pub fn add_response(&mut self, response: TransportResponse) {
        self.responses.lock().unwrap().push_back(response);
    }

    /// Add multiple responses to the queue
    #[allow(dead_code)]
    pub fn add_responses(&mut self, responses: Vec<TransportResponse>) {
        let mut queue = self.responses.lock().unwrap();
        for response in responses {
            queue.push_back(response);
        }
    }

    /// Get all recorded calls
    pub fn get_calls(&self) -> Vec<TransportCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Clear all recorded calls
    #[allow(dead_code)]
    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    /// Get the next response from the queue, or return a default error
    fn get_next_response(&self) -> TransportResponse {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(TransportResponse::SendErr(TransportError::NotConnected))
    }
}

impl Default for StubTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportOps for StubTransport {
    fn send<'a>(
        &'a mut self,
        buffers: &'a [IoSlice<'a>],
    ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + Send + 'a>> {
        Box::pin(async move {
            // Collect data from buffers
            let mut data = Vec::new();
            for buffer in buffers {
                data.extend_from_slice(buffer);
            }

            // Record the call
            self.calls
                .lock()
                .unwrap()
                .push(TransportCall::Send { data });

            // Return the configured response
            match self.get_next_response() {
                TransportResponse::SendOk => Ok(()),
                TransportResponse::SendErr(err) => Err(err),
                _ => Err(TransportError::NotConnected), // Default error for unexpected response
            }
        })
    }

    fn recv<'a>(
        &'a mut self,
        buffer: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = Result<usize, TransportError>> + Send + 'a>> {
        Box::pin(async move {
            let buffer_size = buffer.len();

            // Record the call
            self.calls
                .lock()
                .unwrap()
                .push(TransportCall::Recv { buffer_size });

            // Return the configured response
            match self.get_next_response() {
                TransportResponse::RecvOk(data) => {
                    let copy_len = std::cmp::min(data.len(), buffer.len());
                    buffer[..copy_len].copy_from_slice(&data[..copy_len]);
                    Ok(copy_len)
                }
                TransportResponse::RecvErr(err) => Err(err),
                _ => Err(TransportError::NotConnected), // Default error for unexpected response
            }
        })
    }

    fn shutdown<'a>(
        &'a mut self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            // Record the call
            self.calls
                .lock()
                .unwrap()
                .push(TransportCall::Shutdown { timeout });

            // Shutdown doesn't return anything, just record the call
        })
    }
}
