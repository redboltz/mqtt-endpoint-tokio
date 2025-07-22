use serde::Serialize;
use std::io::Cursor;
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
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::connection::event::TimerKind;
use mqtt_protocol_core::mqtt::connection::role::RoleType;
use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent, Sendable};
use mqtt_protocol_core::mqtt::packet::GenericPacketTrait;
use mqtt_protocol_core::mqtt::packet::{GenericPacket, PacketType};
use mqtt_protocol_core::mqtt::types::IsPacketId;
use std::hash::Hash;

/// Packet filter for selective receiving
#[derive(Debug, Clone)]
pub enum PacketFilter {
    /// Accept packets of any of these types
    Include(Vec<PacketType>),
    /// Reject packets of any of these types (accept all others)
    Exclude(Vec<PacketType>),
    /// Accept all packets (no filtering)
    Any,
}

impl PacketFilter {
    /// Check if a packet matches this filter
    pub fn matches<PacketIdType>(&self, packet: &GenericPacket<PacketIdType>) -> bool
    where
        PacketIdType: IsPacketId + Serialize,
    {
        match self {
            PacketFilter::Include(types) => types.contains(&packet.packet_type()),
            PacketFilter::Exclude(types) => !types.contains(&packet.packet_type()),
            PacketFilter::Any => true,
        }
    }

    /// Create an include filter for specific packet types
    pub fn include(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Include(types.into())
    }

    /// Create an exclude filter for specific packet types
    pub fn exclude(types: impl Into<Vec<PacketType>>) -> Self {
        PacketFilter::Exclude(types.into())
    }
}

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
    Send {
        packet: Box<dyn SendableErased<Role, PacketIdType>>,
        response_tx: oneshot::Sender<Result<Vec<GenericEvent<PacketIdType>>, SendError>>,
    },
    Recv {
        filter: PacketFilter,
        response_tx: oneshot::Sender<Result<GenericPacket<PacketIdType>, SendError>>,
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
            let mut connection: GenericConnection<Role, PacketIdType> =
                GenericConnection::new(version);
            let mut stream = stream;
            let mut pingreq_send_timer: Option<tokio::task::JoinHandle<()>> = None;
            let mut pingreq_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
            let mut pingresp_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
            let (timer_tx, mut timer_rx) = mpsc::unbounded_channel::<TimerKind>();

            let mut read_buffer = vec![0u8; 4096];

            loop {
                tokio::select! {
                    // Handle requests from external API
                    request = rx_send.recv() => {
                        match request {
                            Some(RequestResponse::Send { packet, response_tx }) => {
                                let events = packet.dispatch_send_boxed(&mut connection);
                                if let Err(_) = response_tx.send(Ok(events.clone())) {
                                    break; // Channel closed, endpoint dropped
                                }
                                // Process events recursively
                                Self::process_events(&mut connection, &mut stream, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, events).await;
                            }
                            Some(RequestResponse::Recv { filter, response_tx }) => {
                                // Read until we get a packet matching the filter
                                loop {
                                    match stream.read(&mut read_buffer).await {
                                        Ok(0) => {
                                            // EOF - connection closed
                                            let _ = response_tx.send(Err(SendError::ConnectionError("Connection closed".to_string())));
                                            break;
                                        }
                                        Ok(n) => {
                                            // Process received bytes
                                            let mut cursor = Cursor::new(&read_buffer[..n]);
                                            let events = connection.recv(&mut cursor);

                                            if events.is_empty() {
                                                // Packet incomplete - need more data
                                                continue;
                                            } else {
                                                // Process events and check for received packet
                                                let received_packet = Self::process_events_with_recv(&mut connection, &mut stream, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, events).await;

                                                if let Some(packet) = received_packet {
                                                    // Check if packet matches the filter
                                                    if filter.matches(&packet) {
                                                        let _ = response_tx.send(Ok(packet));
                                                        break;
                                                    } else {
                                                        // Packet doesn't match filter - discard and continue reading
                                                        continue;
                                                    }
                                                } else {
                                                    // No packet received in this batch - continue reading
                                                    continue;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            // Read error
                                            let _ = response_tx.send(Err(SendError::ConnectionError(e.to_string())));
                                            break;
                                        }
                                    }
                                }
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
                                // Process events recursively
                                Self::process_events(&mut connection, &mut stream, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, events).await;
                            }
                            None => break, // Channel closed, endpoint dropped
                        }
                    }

                    // Handle timer expiration
                    timer_kind = timer_rx.recv() => {
                        if let Some(kind) = timer_kind {
                            // Timer has fired - clear the corresponding timer
                            match kind {
                                TimerKind::PingreqSend => pingreq_send_timer = None,
                                TimerKind::PingreqRecv => pingreq_recv_timer = None,
                                TimerKind::PingrespRecv => pingresp_recv_timer = None,
                            }
                            let events = connection.notify_timer_fired(kind);
                            // Process events recursively
                            Self::process_events(&mut connection, &mut stream, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, events).await;
                        }
                    }

                }
            }

            // Cancel all timers when event loop exits
            if let Some(handle) = pingreq_send_timer {
                handle.abort();
            }
            if let Some(handle) = pingreq_recv_timer {
                handle.abort();
            }
            if let Some(handle) = pingresp_recv_timer {
                handle.abort();
            }
        });

        Self {
            tx_send,
            _marker: PhantomData,
        }
    }

    async fn process_events_with_recv<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        stream: &mut S,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        events: Vec<GenericEvent<PacketIdType>>,
    ) -> Option<GenericPacket<PacketIdType>>
    where
        S: AsyncWrite + Unpin,
    {
        let mut received_packet = None;

        for event in events {
            match event {
                GenericEvent::NotifyPacketReceived(packet) => {
                    // Store the received packet (should be only one per events batch)
                    received_packet = Some(packet);
                }
                _ => {
                    // Process other events as before
                    Self::process_single_event(
                        connection,
                        stream,
                        pingreq_send_timer,
                        pingreq_recv_timer,
                        pingresp_recv_timer,
                        timer_tx,
                        event,
                    )
                    .await;
                }
            }
        }

        received_packet
    }

    async fn process_events<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        stream: &mut S,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        events: Vec<GenericEvent<PacketIdType>>,
    ) where
        S: AsyncWrite + Unpin,
    {
        for event in events {
            Self::process_single_event(
                connection,
                stream,
                pingreq_send_timer,
                pingreq_recv_timer,
                pingresp_recv_timer,
                timer_tx,
                event,
            )
            .await;
        }
    }

    async fn process_single_event<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        stream: &mut S,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        event: GenericEvent<PacketIdType>,
    ) where
        S: AsyncWrite + Unpin,
    {
        match event {
            GenericEvent::RequestSendPacket {
                packet,
                release_packet_id_if_send_error,
            } => {
                // Get buffers from packet
                let buffers = packet.to_buffers();
                let total_len = packet.size();

                // Send using vectored I/O
                let send_result = match stream.write_vectored(&buffers).await {
                    Ok(bytes_written) => {
                        // Verify all data was written
                        if bytes_written == total_len {
                            Ok(())
                        } else {
                            // Partial write - treat as error for MQTT packet integrity
                            Err(std::io::Error::new(
                                std::io::ErrorKind::WriteZero,
                                format!("Partial write: {}/{} bytes", bytes_written, total_len),
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
                                // Process sub-events using boxed future to avoid recursion limit
                                Box::pin(Self::process_events(
                                    connection,
                                    stream,
                                    pingreq_send_timer,
                                    pingreq_recv_timer,
                                    pingresp_recv_timer,
                                    timer_tx,
                                    release_events,
                                ))
                                .await;
                            }
                        }
                    }
                    Err(_) => {
                        // Send failed, release packet ID if needed
                        if let Some(packet_id) = release_packet_id_if_send_error {
                            let release_events = connection.release_packet_id(packet_id);
                            // Process sub-events using boxed future to avoid recursion limit
                            Box::pin(Self::process_events(
                                connection,
                                stream,
                                pingreq_send_timer,
                                pingreq_recv_timer,
                                pingresp_recv_timer,
                                timer_tx,
                                release_events,
                            ))
                            .await;
                        }
                    }
                }
            }

            GenericEvent::NotifyPacketIdReleased(_packet_id) => {
                // Currently do nothing as specified
            }

            GenericEvent::RequestTimerReset { kind, duration_ms } => {
                // Cancel existing timer if present and set new timer
                match kind {
                    TimerKind::PingreqSend => {
                        if let Some(handle) = pingreq_send_timer.take() {
                            handle.abort();
                        }
                        let timer_tx_clone = timer_tx.clone();
                        let handle = tokio::spawn(async move {
                            sleep(Duration::from_millis(duration_ms)).await;
                            let _ = timer_tx_clone.send(kind);
                        });
                        *pingreq_send_timer = Some(handle);
                    }
                    TimerKind::PingreqRecv => {
                        if let Some(handle) = pingreq_recv_timer.take() {
                            handle.abort();
                        }
                        let timer_tx_clone = timer_tx.clone();
                        let handle = tokio::spawn(async move {
                            sleep(Duration::from_millis(duration_ms)).await;
                            let _ = timer_tx_clone.send(kind);
                        });
                        *pingreq_recv_timer = Some(handle);
                    }
                    TimerKind::PingrespRecv => {
                        if let Some(handle) = pingresp_recv_timer.take() {
                            handle.abort();
                        }
                        let timer_tx_clone = timer_tx.clone();
                        let handle = tokio::spawn(async move {
                            sleep(Duration::from_millis(duration_ms)).await;
                            let _ = timer_tx_clone.send(kind);
                        });
                        *pingresp_recv_timer = Some(handle);
                    }
                }
            }

            GenericEvent::RequestTimerCancel(kind) => {
                // Cancel timer if present
                match kind {
                    TimerKind::PingreqSend => {
                        if let Some(handle) = pingreq_send_timer.take() {
                            handle.abort();
                        }
                    }
                    TimerKind::PingreqRecv => {
                        if let Some(handle) = pingreq_recv_timer.take() {
                            handle.abort();
                        }
                    }
                    TimerKind::PingrespRecv => {
                        if let Some(handle) = pingresp_recv_timer.take() {
                            handle.abort();
                        }
                    }
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
                // This should not happen in process_single_event
                // NotifyPacketReceived is handled separately in process_events_with_recv
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
            .send(RequestResponse::Send {
                packet: Box::new(packet),
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Receive any MQTT packet
    pub async fn recv(&self) -> Result<GenericPacket<PacketIdType>, SendError> {
        self.recv_filtered(PacketFilter::Any).await
    }

    /// Receive MQTT packet matching the specified filter
    pub async fn recv_filtered(
        &self,
        filter: PacketFilter,
    ) -> Result<GenericPacket<PacketIdType>, SendError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx_send
            .send(RequestResponse::Recv {
                filter,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Acquire a unique packet ID
    pub async fn acquire_unique_packet_id(&self) -> Result<PacketIdType, SendError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx_send
            .send(RequestResponse::AcquirePacketId { response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Register a packet ID as in use
    pub async fn register_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx_send
            .send(RequestResponse::RegisterPacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Release a packet ID
    pub async fn release_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let (response_tx, response_rx) = oneshot::channel();

        self.tx_send
            .send(RequestResponse::ReleasePacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }
}

pub type Endpoint<Role> = GenericEndpoint<Role, u16>;
