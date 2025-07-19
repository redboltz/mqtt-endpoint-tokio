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
use std::marker::PhantomData;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::time::sleep;
use serde::Serialize;

use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent, Sendable};
use mqtt_protocol_core::mqtt::connection::role::RoleType;
use mqtt_protocol_core::mqtt::connection::event::TimerKind;
use mqtt_protocol_core::mqtt::packet::GenericPacketTrait;
use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::packet::GenericPacket;
use mqtt_protocol_core::mqtt::types::IsPacketId;
use std::hash::Hash;

pub struct GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    tx_send: mpsc::UnboundedSender<RequestResponse<Role, PacketIdType>>,
    _marker: PhantomData<Role>,
}

enum RequestResponse<Role, PacketIdType>
where
    Role: RoleType,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    Sendable {
        packet: Box<dyn SendableErased<Role, PacketIdType>>,
        response_tx: oneshot::Sender<Result<Vec<GenericEvent<PacketIdType>>, SendError>>,
    },
    AcquirePacketId {
        response_tx: oneshot::Sender<Result<PacketIdType, SendError>>,
    },
    RegisterPacketId {
        packet_id: PacketIdType,
        response_tx: oneshot::Sender<Result<(), SendError>>,
    },
    ReleasePacketId {
        packet_id: PacketIdType,
        response_tx: oneshot::Sender<Result<(), SendError>>,
    },
}

/// Type-erased trait for sending Sendable packets through channels
trait SendableErased<Role, PacketIdType>: Send
where
    Role: RoleType,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + 'static,
{
    fn dispatch_send_boxed(
        self: Box<Self>,
        connection: &mut GenericConnection<Role, PacketIdType>,
    ) -> Vec<GenericEvent<PacketIdType>>;
}

impl<T, Role, PacketIdType> SendableErased<Role, PacketIdType> for T
where
    T: Sendable<Role, PacketIdType> + Send,
    Role: RoleType,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + 'static,
{
    fn dispatch_send_boxed(
        self: Box<Self>,
        connection: &mut GenericConnection<Role, PacketIdType>,
    ) -> Vec<GenericEvent<PacketIdType>> {
        (*self).dispatch_send(connection)
    }
}


#[derive(Debug, Clone)]
pub enum SendError {
    ChannelClosed,
    ConnectionError(String),
}

impl<Role, PacketIdType> GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
    <PacketIdType as IsPacketId>::Buffer: Send,
{
    pub fn new<S>(version: Version, stream: S) -> Self 
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (tx_send, mut rx_send) = mpsc::unbounded_channel();
        
        tokio::spawn(async move {
            let mut connection: GenericConnection<Role, PacketIdType> = GenericConnection::new(version);
            let mut stream = stream;
            let mut timers: Vec<(TimerKind, tokio::task::JoinHandle<()>)> = Vec::new();
            let (timer_tx, mut timer_rx) = mpsc::unbounded_channel::<TimerKind>();
            let mut event_queue: Vec<GenericEvent<PacketIdType>> = Vec::new();
            
            loop {
                // Process any queued events first
                while let Some(event) = event_queue.pop() {
                    Self::process_single_event(&mut connection, &mut stream, &mut timers, &timer_tx, &mut event_queue, event).await;
                }
                
                tokio::select! {
                    // Handle requests from external API
                    request = rx_send.recv() => {
                        match request {
                            Some(RequestResponse::Sendable { packet, response_tx }) => {
                                let events = packet.dispatch_send_boxed(&mut connection);
                                if let Err(_) = response_tx.send(Ok(events.clone())) {
                                    break; // Channel closed, endpoint dropped
                                }
                                // Queue events for processing
                                event_queue.extend(events);
                            }
                            Some(RequestResponse::AcquirePacketId { response_tx }) => {
                                match connection.acquire_unique_packet_id() {
                                    Ok(packet_id) => {
                                        let _ = response_tx.send(Ok(packet_id));
                                    }
                                    Err(e) => {
                                        let _ = response_tx.send(Err(SendError::ConnectionError(e.to_string())));
                                    }
                                }
                            }
                            Some(RequestResponse::RegisterPacketId { packet_id, response_tx }) => {
                                match connection.register_packet_id(packet_id) {
                                    Ok(()) => {
                                        let _ = response_tx.send(Ok(()));
                                    }
                                    Err(e) => {
                                        let _ = response_tx.send(Err(SendError::ConnectionError(e.to_string())));
                                    }
                                }
                            }
                            Some(RequestResponse::ReleasePacketId { packet_id, response_tx }) => {
                                let events = connection.release_packet_id(packet_id);
                                let _ = response_tx.send(Ok(()));
                                // Queue events for processing
                                event_queue.extend(events);
                            }
                            None => break, // Channel closed, endpoint dropped
                        }
                    }
                    
                    // Handle timer expiration
                    timer_kind = timer_rx.recv() => {
                        if let Some(kind) = timer_kind {
                            // Timer has fired - remove it from active timers
                            timers.retain(|(timer_kind, _)| *timer_kind != kind);
                            let events = connection.notify_timer_fired(kind);
                            // Queue events for processing
                            event_queue.extend(events);
                        }
                    }
                }
            }
            
            // Cancel all timers when event loop exits
            for (_, handle) in timers {
                handle.abort();
            }
        });

        Self {
            tx_send,
            _marker: PhantomData,
        }
    }

    async fn process_single_event<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        stream: &mut S,
        timers: &mut Vec<(TimerKind, tokio::task::JoinHandle<()>)>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        event_queue: &mut Vec<GenericEvent<PacketIdType>>,
        event: GenericEvent<PacketIdType>,
    ) 
    where
        S: AsyncWrite + Unpin,
    {
        match event {
            GenericEvent::RequestSendPacket { packet, release_packet_id_if_send_error } => {
                // Get buffers from packet
                let buffers = packet.to_buffers();
                
                // Send using vectored I/O
                let send_result = match stream.write_vectored(&buffers).await {
                    Ok(bytes_written) => {
                        // Verify all data was written
                        let total_len: usize = buffers.iter().map(|b| b.len()).sum();
                        if bytes_written == total_len {
                            Ok(())
                        } else {
                            // Partial write - treat as error for MQTT packet integrity
                            Err(std::io::Error::new(
                                std::io::ErrorKind::WriteZero, 
                                format!("Partial write: {}/{} bytes", bytes_written, total_len)
                            ))
                        }
                    }
                    Err(e) => Err(e),
                };
                
                match send_result {
                    Ok(_) => {
                        // Successfully sent, flush the stream
                        if let Err(_) = stream.flush().await {
                            // Flush failed, release packet ID if needed
                            if let Some(packet_id) = release_packet_id_if_send_error {
                                let release_events = connection.release_packet_id(packet_id);
                                event_queue.extend(release_events);
                            }
                        }
                    }
                    Err(_) => {
                        // Send failed, release packet ID if needed
                        if let Some(packet_id) = release_packet_id_if_send_error {
                            let release_events = connection.release_packet_id(packet_id);
                            event_queue.extend(release_events);
                        }
                    }
                }
            }
            
            GenericEvent::NotifyPacketIdReleased(_packet_id) => {
                // Currently do nothing as specified
            }
            
            GenericEvent::RequestTimerReset { kind, duration_ms } => {
                // Cancel existing timer if present
                if let Some(pos) = timers.iter().position(|(timer_kind, _)| *timer_kind == kind) {
                    let (_, handle) = timers.remove(pos);
                    handle.abort();
                }
                
                // Set new timer
                let timer_tx_clone = timer_tx.clone();
                let handle = tokio::spawn(async move {
                    sleep(Duration::from_millis(duration_ms)).await;
                    let _ = timer_tx_clone.send(kind);
                });
                timers.push((kind, handle));
            }
            
            GenericEvent::RequestTimerCancel(kind) => {
                // Cancel timer if present
                if let Some(pos) = timers.iter().position(|(timer_kind, _)| *timer_kind == kind) {
                    let (_, handle) = timers.remove(pos);
                    handle.abort();
                }
            }
            
            GenericEvent::RequestClose => {
                // Shutdown the stream - for most stream types, this means
                // dropping the stream or calling shutdown if available
                let _ = stream.shutdown().await;
                // Note: In a real implementation, we would signal the main loop to exit
            }
            
            GenericEvent::NotifyError(_error) => {
                // Handle error - could log or trigger connection closure
                // For now, continue processing
            }
            
            GenericEvent::NotifyPacketReceived(_packet) => {
                // This would be handled by a separate receive loop
                // in a full implementation
            }
        }
    }

    /// Send MQTT packet with compile-time type safety
    ///
    /// This method accepts any packet type that implements `Sendable<Role, PacketIdType>` 
    /// for compile-time verification, or `GenericPacket<PacketIdType>` for dynamic cases.
    /// All packets are converted to GenericPacket internally via the Into trait.
    pub async fn send<T>(&self, packet: T) -> Result<Vec<GenericEvent<PacketIdType>>, SendError>
    where
        T: Into<GenericPacket<PacketIdType>> + Sendable<Role, PacketIdType> + Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(RequestResponse::Sendable {
                packet: Box::new(packet),
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

    /// Acquire a unique packet ID
    pub async fn acquire_unique_packet_id(&self) -> Result<PacketIdType, SendError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(RequestResponse::AcquirePacketId { response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

    /// Register a packet ID as in use
    pub async fn register_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(RequestResponse::RegisterPacketId { packet_id, response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

    /// Release a packet ID
    pub async fn release_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(RequestResponse::ReleasePacketId { packet_id, response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

}

pub type Endpoint<Role> = GenericEndpoint<Role, u16>;