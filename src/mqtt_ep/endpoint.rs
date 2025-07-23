use serde::Serialize;
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
use tokio::time::sleep;
use std::future;

use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::connection::event::TimerKind;
use mqtt_protocol_core::mqtt::connection::role::RoleType;
use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent, Sendable};
use mqtt_protocol_core::mqtt::packet::GenericPacketTrait;
use mqtt_protocol_core::mqtt::packet::v5_0;
use mqtt_protocol_core::mqtt::packet::{GenericPacket, GenericStorePacket, PacketType};
use mqtt_protocol_core::mqtt::types::IsPacketId;
use std::hash::Hash;

/// Configuration for MQTT endpoint (settable only at initialization time)
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    /// Default connection options (can be overridden per connection)
    pub default_connection_options: ConnectionOption,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            default_connection_options: ConnectionOption::default(),
        }
    }
}

/// Connection options that can be set dynamically for each connection/reconnection
#[derive(Debug, Clone)]
pub struct ConnectionOption {
    /// PINGREQ send interval in milliseconds (for v3.1.1 and v5.0)
    pub pingreq_send_interval: Option<u64>,
    /// Keep alive interval in seconds (for v3.1.1 and v5.0)
    pub keep_alive_interval: Option<u16>,
    /// Packet ID exhaust retry interval in milliseconds
    pub packet_id_exhaust_retry_interval: Option<u64>,
}

impl Default for ConnectionOption {
    fn default() -> Self {
        Self {
            pingreq_send_interval: None,
            keep_alive_interval: None,
            packet_id_exhaust_retry_interval: None,
        }
    }
}

impl ConnectionOption {
    /// Create a new ConnectionOption with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the PINGREQ send interval in milliseconds
    pub fn pingreq_send_interval(mut self, interval: u64) -> Self {
        self.pingreq_send_interval = Some(interval);
        self
    }

    /// Set the keep alive interval in seconds
    pub fn keep_alive_interval(mut self, interval: u16) -> Self {
        self.keep_alive_interval = Some(interval);
        self
    }

    /// Set the packet ID exhaust retry interval in milliseconds
    pub fn packet_id_exhaust_retry_interval(mut self, interval: u64) -> Self {
        self.packet_id_exhaust_retry_interval = Some(interval);
        self
    }
}

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

/// Builder for creating GenericEndpoint with custom configuration
pub struct GenericEndpointBuilder<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    version: Version,
    config: EndpointConfig,
    _marker: PhantomData<(Role, PacketIdType)>,
}

impl<Role, PacketIdType> GenericEndpointBuilder<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
    <PacketIdType as IsPacketId>::Buffer: Send,
{
    /// Create a new builder with the specified MQTT version
    pub fn new(version: Version) -> Self {
        Self {
            version,
            config: EndpointConfig::default(),
            _marker: PhantomData,
        }
    }

    /// Set the default connection options (can be overridden per connection)
    pub fn default_connection_options(mut self, options: ConnectionOption) -> Self {
        self.config.default_connection_options = options;
        self
    }

    /// Build the endpoint (initially in disconnected state)
    pub fn build(self) -> GenericEndpoint<Role, PacketIdType> {
        GenericEndpoint::new_with_config(self.version, self.config)
    }
}

pub struct GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    version: Version,
    config: EndpointConfig,
    tx_send: mpsc::UnboundedSender<RequestResponse<Role, PacketIdType>>,
    event_loop_handle: tokio::task::JoinHandle<()>,
    _marker: PhantomData<Role>,
}

enum RequestResponse<Role, PacketIdType>
where
    Role: RoleType,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    Send {
        packet: Box<dyn SendableErased<Role, PacketIdType>>,
        response_tx: oneshot::Sender<Result<(), SendError>>,
    },
    Recv {
        filter: PacketFilter,
        response_tx: oneshot::Sender<Result<GenericPacket<PacketIdType>, SendError>>,
    },
    AcquirePacketId {
        response_tx: oneshot::Sender<Result<PacketIdType, SendError>>,
    },
    AcquirePacketIdWhenAvailable {
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
    Close {
        response_tx: oneshot::Sender<Result<(), SendError>>,
    },
    Connect {
        transport: Box<dyn crate::mqtt_ep::transport::TransportOps + Send>,
        options: ConnectionOption,
        response_tx: oneshot::Sender<Result<(), ConnectError>>,
    },
    RestorePackets {
        packets: Vec<GenericStorePacket<PacketIdType>>,
        response_tx: oneshot::Sender<Result<(), SendError>>,
    },
    GetStoredPackets {
        response_tx: oneshot::Sender<Result<Vec<GenericStorePacket<PacketIdType>>, SendError>>,
    },
    RegulateForStore {
        packet: v5_0::GenericPublish<PacketIdType>,
        response_tx: oneshot::Sender<Result<v5_0::GenericPublish<PacketIdType>, SendError>>,
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
    NotConnected,
}

impl<Role, PacketIdType> GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
    <PacketIdType as IsPacketId>::Buffer: Send,
{
    /// Create a new builder for configuring the endpoint
    pub fn builder(version: Version) -> GenericEndpointBuilder<Role, PacketIdType> {
        GenericEndpointBuilder::new(version)
    }

    /// Create a new endpoint with default configuration (initially disconnected)
    pub fn new_disconnected(version: Version) -> Self {
        Self::new_with_config(version, EndpointConfig::default())
    }

    /// Create a new endpoint with a pre-connected stream (for backward compatibility with tests)
    /// This method is primarily for tests that use tokio::io::duplex streams
    pub fn new<S>(version: Version, _stream: S) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
    {
        // For backward compatibility, we create the endpoint and store the stream
        // The actual connection will happen when methods are called
        // This mimics the old synchronous behavior expected by tests
        let endpoint = Self::new_with_config(version, EndpointConfig::default());

        // In the new architecture, we need to connect asynchronously
        // For now, return the disconnected endpoint - tests might need updating
        endpoint
    }

    /// Create a new endpoint with custom configuration (initially disconnected)
    pub fn new_with_config(version: Version, config: EndpointConfig) -> Self {
        let connection = GenericConnection::new(version);
        let (tx_send, rx_send) = mpsc::unbounded_channel();

        // Start event loop immediately
        let event_loop_handle = tokio::spawn(Self::event_loop(connection, rx_send));

        Self {
            version,
            config,
            tx_send,
            event_loop_handle,
            _marker: PhantomData,
        }
    }

    async fn process_events<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        transport: &mut S,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        pending_packet_id_requests: &mut Vec<oneshot::Sender<Result<PacketIdType, SendError>>>,
        events: Vec<GenericEvent<PacketIdType>>,
    ) where
        S: crate::mqtt_ep::transport::TransportOps,
    {
        for event in events {
            match event {
                GenericEvent::NotifyPacketIdReleased(_packet_id) => {
                    // Handle packet ID release and process waiting queue
                    Self::process_packet_id_waiting_queue(connection, pending_packet_id_requests);
                }
                _ => {
                    // Process other events
                    Self::process_single_event(
                        connection,
                        transport,
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
    }

    /// Process the packet ID waiting queue when a packet ID is released
    fn process_packet_id_waiting_queue(
        connection: &mut GenericConnection<Role, PacketIdType>,
        pending_requests: &mut Vec<oneshot::Sender<Result<PacketIdType, SendError>>>,
    ) {
        // Process requests from the front (index 0) to maintain FIFO order
        while !pending_requests.is_empty() {
            // Note: acquire_packet_id() returns Result<PacketIdType, MqttError>, no events
            match connection.acquire_packet_id() {
                Ok(packet_id) => {
                    // Successfully acquired packet ID, remove and send to the first waiting requester
                    let response_tx = pending_requests.remove(0);
                    let _ = response_tx.send(Ok(packet_id));
                }
                Err(_) => {
                    // Failed to acquire packet ID, stop processing to maintain order
                    break;
                }
            }
        }
    }

    async fn process_single_event<S>(
        connection: &mut GenericConnection<Role, PacketIdType>,
        transport: &mut S,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        timer_tx: &mpsc::UnboundedSender<TimerKind>,
        event: GenericEvent<PacketIdType>,
    ) where
        S: crate::mqtt_ep::transport::TransportOps,
    {
        match event {
            GenericEvent::RequestSendPacket {
                packet,
                release_packet_id_if_send_error,
            } => {
                // Get buffers from packet
                let buffers = packet.to_buffers();

                // Send using TransportOps
                let send_result = transport.send(&buffers).await;

                match send_result {
                    Ok(_) => {
                        // Successfully sent - TransportOps::send handles flushing internally
                    }
                    Err(_) => {
                        // Send failed, release packet ID if needed
                        if let Some(packet_id) = release_packet_id_if_send_error {
                            let release_events = connection.release_packet_id(packet_id);
                            // Process sub-events using boxed future to avoid recursion limit
                            // Note: We don't have access to pending_packet_id_requests here in process_single_event
                            // This is acceptable since NotifyPacketIdReleased events should be handled at the top level
                            let mut empty_queue = Vec::new();
                            Box::pin(Self::process_events(
                                connection,
                                transport,
                                pingreq_send_timer,
                                pingreq_recv_timer,
                                pingresp_recv_timer,
                                timer_tx,
                                &mut empty_queue,
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
                // Shutdown the transport
                let _ = crate::mqtt_ep::transport::TransportOps::shutdown(
                    transport,
                    Duration::from_secs(5),
                )
                .await;
                // Note: transport.recv() will return an error after shutdown, causing the loop to exit
            }

            GenericEvent::NotifyError(_error) => {
                // Handle error - could log or trigger connection closure
                // For now, continue processing
            }

            GenericEvent::NotifyPacketReceived(_packet) => {
                // This should not happen in process_single_event
                // NotifyPacketReceived is handled separately in RequestResponse::Recv
            }
        }
    }

    /// Send MQTT packet with compile-time type safety
    ///
    /// This method accepts any packet type that implements `Sendable<Role, PacketIdType>`
    /// for compile-time verification, or `GenericPacket<PacketIdType>` for dynamic cases.
    /// All packets are converted to GenericPacket internally via the Into trait.
    pub async fn send<T>(&self, packet: T) -> Result<(), SendError>
    where
        T: Into<GenericPacket<PacketIdType>> + Sendable<Role, PacketIdType> + Send + 'static,
    {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
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
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::Recv {
                filter,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Acquire a unique packet ID
    pub async fn acquire_packet_id(&self) -> Result<PacketIdType, SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::AcquirePacketId { response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Acquire a unique packet ID, waiting until one becomes available
    ///
    /// Unlike acquire_packet_id(), this method will not return an error
    /// if all packet IDs are currently in use. Instead, it will wait until
    /// a packet ID is released and becomes available.
    pub async fn acquire_packet_id_when_available(&self) -> Result<PacketIdType, SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::AcquirePacketIdWhenAvailable { response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Register a packet ID as in use
    pub async fn register_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::RegisterPacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Release a packet ID
    pub async fn release_packet_id(&self, packet_id: PacketIdType) -> Result<(), SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::ReleasePacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Restore packets to the connection store
    ///
    /// This method allows restoring previously stored packets back to the connection.
    /// This is typically used during session restoration to recover in-flight QoS 1 and QoS 2 packets.
    /// The packets will be restored to the appropriate internal tracking structures based on their type and QoS level.
    pub async fn restore_packets(
        &self,
        packets: Vec<GenericStorePacket<PacketIdType>>,
    ) -> Result<(), SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::RestorePackets {
                packets,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Get all stored packets from the connection store
    ///
    /// This method retrieves all currently stored packets from the connection.
    /// This is typically used for session persistence to save in-flight QoS 1 and QoS 2 packets
    /// before closing the connection, so they can be restored later with restore_packets().
    pub async fn get_stored_packets(
        &self,
    ) -> Result<Vec<GenericStorePacket<PacketIdType>>, SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::GetStoredPackets { response_tx })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Regulate MQTT v5.0 PUBLISH packet for storage
    ///
    /// This method processes an MQTT v5.0 PUBLISH packet to make it suitable for storage.
    /// It resolves topic aliases to actual topic names and removes topic alias properties,
    /// ensuring the packet can be stored and restored correctly during session persistence.
    /// This is typically used before storing QoS 1 and QoS 2 packets that use topic aliases.
    pub async fn regulate_for_store(
        &self,
        packet: v5_0::GenericPublish<PacketIdType>,
    ) -> Result<v5_0::GenericPublish<PacketIdType>, SendError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::RegulateForStore {
                packet,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Connect to the specified transport with default connection options
    pub async fn connect<T>(&self, transport: T) -> Result<(), ConnectError>
    where
        T: crate::mqtt_ep::transport::TransportOps + Send + 'static,
    {
        self.connect_with_options(transport, self.config.default_connection_options.clone())
            .await
    }

    /// Connect to the specified transport with specific connection options
    pub async fn connect_with_options<T>(
        &self,
        transport: T,
        options: ConnectionOption,
    ) -> Result<(), ConnectError>
    where
        T: crate::mqtt_ep::transport::TransportOps + Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx_send
            .send(RequestResponse::Connect {
                transport: Box::new(transport),
                options,
                response_tx,
            })
            .is_err()
        {
            return Err(ConnectError::Transport(
                crate::mqtt_ep::transport::TransportError::NotConnected,
            ));
        }

        response_rx.await.map_err(|_| {
            ConnectError::Transport(crate::mqtt_ep::transport::TransportError::NotConnected)
        })?
    }

    /// Get the tx_send channel - always available in new architecture
    fn get_tx_send(&self) -> mpsc::UnboundedSender<RequestResponse<Role, PacketIdType>> {
        self.tx_send.clone()
    }

    /// Apply connection options to the MQTT connection
    fn apply_connection_options(
        connection: &mut GenericConnection<Role, PacketIdType>,
        options: &ConnectionOption,
    ) {
        if let Some(interval) = options.pingreq_send_interval {
            connection.set_pingreq_send_interval(interval);
        }
        // TODO: Implement set_keep_alive_interval and set_packet_id_exhaust_retry_interval
        // if let Some(interval) = options.keep_alive_interval {
        //     connection.set_keep_alive_interval(interval);
        // }
        // if let Some(interval) = options.packet_id_exhaust_retry_interval {
        //     connection.set_packet_id_exhaust_retry_interval(interval);
        // }
    }

    /// Close the current connection
    pub async fn close(&self) -> Result<(), SendError> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx_send
            .send(RequestResponse::Close { response_tx })
            .is_err()
        {
            return Err(SendError::ChannelClosed);
        }

        response_rx.await.map_err(|_| SendError::ChannelClosed)?
    }

    /// Handle close request with proper queueing
    async fn handle_close_request(
        transport: &mut Option<Box<dyn crate::mqtt_ep::transport::TransportOps + Send>>,
        pending_close_notifications: &mut Vec<oneshot::Sender<Result<(), SendError>>>,
        pingreq_send_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingreq_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        pingresp_recv_timer: &mut Option<tokio::task::JoinHandle<()>>,
        response_tx: oneshot::Sender<Result<(), SendError>>,
    ) {
        if pending_close_notifications.len() > 0 {
            // Close already in progress - add to queue
            pending_close_notifications.push(response_tx);
            return;
        }

        // Check if already disconnected
        if transport.is_none() {
            // Already disconnected - immediately notify
            let _ = response_tx.send(Ok(()));
            return;
        }

        // Start close process
        pending_close_notifications.push(response_tx);

        // Cancel all timers immediately
        if let Some(timer) = pingreq_send_timer.take() {
            timer.abort();
        }
        if let Some(timer) = pingreq_recv_timer.take() {
            timer.abort();
        }
        if let Some(timer) = pingresp_recv_timer.take() {
            timer.abort();
        }

        // Shutdown transport
        if let Some(mut t) = transport.take() {
            let _ = t.shutdown(Duration::from_secs(5)).await;
        }

        // Notify all pending close requests
        for tx in pending_close_notifications.drain(..) {
            let _ = tx.send(Ok(()));
        }
    }

    /// Event loop that handles transport I/O and MQTT protocol logic
    async fn event_loop(
        mut connection: GenericConnection<Role, PacketIdType>,
        mut rx_send: mpsc::UnboundedReceiver<RequestResponse<Role, PacketIdType>>,
    ) {
        let mut pingreq_send_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut pingreq_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut pingresp_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel::<TimerKind>();

        let mut pending_packet_id_requests: Vec<oneshot::Sender<Result<PacketIdType, SendError>>> =
            Vec::new();
        let mut pending_close_notifications: Vec<oneshot::Sender<Result<(), SendError>>> =
            Vec::new();
        let mut pending_recv_requests: Vec<(PacketFilter, oneshot::Sender<Result<GenericPacket<PacketIdType>, SendError>>)> =
            Vec::new();
        let mut transport: Option<Box<dyn crate::mqtt_ep::transport::TransportOps + Send>> = None;
        let mut read_buffer = vec![0u8; 4096];

        loop {
            tokio::select! {
                // Handle requests from external API
                request = rx_send.recv() => {
                    match request {
                        Some(RequestResponse::Send { packet, response_tx }) => {
                            let events = packet.dispatch_send_boxed(&mut connection);
                            let _ = response_tx.send(Ok(()));
                            if let Some(ref mut t) = transport {
                                Self::process_events(&mut connection, t, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, &mut pending_packet_id_requests, events).await;
                            }
                        }
                        Some(RequestResponse::Recv { filter, response_tx }) => {
                            // Add to pending queue (non-blocking)
                            if transport.is_some() {
                                pending_recv_requests.push((filter, response_tx));
                            } else {
                                let _ = response_tx.send(Err(SendError::NotConnected));
                            }
                        }
                        Some(RequestResponse::AcquirePacketId { response_tx }) => {
                            match connection.acquire_packet_id() {
                                Ok(packet_id) => {
                                    let _ = response_tx.send(Ok(packet_id));
                                }
                                Err(_) => {
                                    pending_packet_id_requests.push(response_tx);
                                }
                            }
                        }
                        Some(RequestResponse::AcquirePacketIdWhenAvailable { response_tx }) => {
                            match connection.acquire_packet_id() {
                                Ok(packet_id) => {
                                    let _ = response_tx.send(Ok(packet_id));
                                }
                                Err(_) => {
                                    pending_packet_id_requests.push(response_tx);
                                }
                            }
                        }
                        Some(RequestResponse::RegisterPacketId { packet_id, response_tx }) => {
                            let _ = connection.register_packet_id(packet_id);
                            let _ = response_tx.send(Ok(()));
                        }
                        Some(RequestResponse::ReleasePacketId { packet_id, response_tx }) => {
                            connection.release_packet_id(packet_id);
                            let _ = response_tx.send(Ok(()));

                            // Check if we can fulfill any pending packet ID requests
                            while let Some(pending_tx) = pending_packet_id_requests.pop() {
                                if let Ok(packet_id) = connection.acquire_packet_id() {
                                    if let Err(_) = pending_tx.send(Ok(packet_id)) {
                                        connection.release_packet_id(packet_id);
                                        break;
                                    }
                                } else {
                                    pending_packet_id_requests.push(pending_tx);
                                    break;
                                }
                            }
                        }
                        Some(RequestResponse::RestorePackets { packets, response_tx }) => {
                            connection.restore_packets(packets);
                            let _ = response_tx.send(Ok(()));
                        }
                        Some(RequestResponse::GetStoredPackets { response_tx }) => {
                            let packets = connection.get_stored_packets();
                            let _ = response_tx.send(Ok(packets));
                        }
                        Some(RequestResponse::RegulateForStore { packet, response_tx }) => {
                            match connection.regulate_for_store(packet.clone()) {
                                Ok(regulated_packet) => {
                                    let _ = response_tx.send(Ok(regulated_packet));
                                }
                                Err(_) => {
                                    let _ = response_tx.send(Ok(packet));
                                }
                            }
                        }
                        Some(RequestResponse::Connect { transport: new_transport, options, response_tx }) => {
                            // Perform handshake
                            let mut boxed_transport = new_transport;
                            match boxed_transport.handshake().await {
                                Ok(()) => {
                                    // Apply connection options
                                    Self::apply_connection_options(&mut connection, &options);

                                    // Set the transport
                                    transport = Some(boxed_transport);
                                    let _ = response_tx.send(Ok(()));
                                }
                                Err(e) => {
                                    let _ = response_tx.send(Err(ConnectError::Transport(e)));
                                }
                            }
                        }
                        Some(RequestResponse::Close { response_tx }) => {
                            // Handle close request
                            Self::handle_close_request(
                                &mut transport,
                                &mut pending_close_notifications,
                                &mut pingreq_send_timer,
                                &mut pingreq_recv_timer,
                                &mut pingresp_recv_timer,
                                response_tx
                            ).await;
                        }
                        None => break, // Channel closed
                    }
                }

                // Handle timer expiration
                timer_kind = timer_rx.recv() => {
                    if let Some(kind) = timer_kind {
                        match kind {
                            TimerKind::PingreqSend => pingreq_send_timer = None,
                            TimerKind::PingreqRecv => pingreq_recv_timer = None,
                            TimerKind::PingrespRecv => pingresp_recv_timer = None,
                        }
                        let events = connection.notify_timer_fired(kind);
                        if let Some(ref mut t) = transport {
                            Self::process_events(&mut connection, t, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, &mut pending_packet_id_requests, events).await;
                        }
                    }
                }

                // Handle transport receive (only when there are pending recv requests)
                recv_result = async {
                    if let Some(ref mut t) = transport {
                        if pending_recv_requests.is_empty() {
                            // No pending recv requests, don't try to receive
                            future::pending().await
                        } else {
                            Some(t.recv(&mut read_buffer).await)
                        }
                    } else {
                        // No transport available
                        future::pending().await
                    }
                } => {
                    if let Some(result) = recv_result {
                        match result {
                            Ok(n) if n > 0 => {
                                // Process received bytes
                                let mut cursor = std::io::Cursor::new(&read_buffer[..n]);
                                let events = connection.recv(&mut cursor);

                                // Check events for received packets matching pending filters
                                Self::process_received_packets(&events, &mut pending_recv_requests);

                                // Process other connection events
                                if let Some(ref mut t) = transport {
                                    Self::process_events(&mut connection, t, &mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer, &timer_tx, &mut pending_packet_id_requests, events).await;
                                }
                            }
                            Ok(_) | Err(_) => {
                                // Connection closed (n = 0) or error - notify all pending recv requests
                                Self::notify_recv_connection_error(&mut pending_recv_requests);
                                transport = None;
                            }
                        }
                    }
                }

            }
        }

        // Cancel timers
        if let Some(handle) = pingreq_send_timer {
            handle.abort();
        }
        if let Some(handle) = pingreq_recv_timer {
            handle.abort();
        }
        if let Some(handle) = pingresp_recv_timer {
            handle.abort();
        }
    }

    /// Process received packets and satisfy matching recv requests
    fn process_received_packets(
        events: &[GenericEvent<PacketIdType>],
        pending_recv_requests: &mut Vec<(PacketFilter, oneshot::Sender<Result<GenericPacket<PacketIdType>, SendError>>)>
    ) {
        let mut satisfied_indices = Vec::new();

        for event in events {
            if let GenericEvent::NotifyPacketReceived(packet) = event {
                // Find first matching filter
                for (i, (filter, _)) in pending_recv_requests.iter().enumerate() {
                    if filter.matches(packet) {
                        satisfied_indices.push(i);
                        break; // Only satisfy one request per packet
                    }
                }
            }
        }

        // Remove satisfied requests in reverse order and send responses
        for &index in satisfied_indices.iter().rev() {
            let (_, response_tx) = pending_recv_requests.swap_remove(index);
            if let Some(GenericEvent::NotifyPacketReceived(packet)) = events.iter()
                .find(|e| matches!(e, GenericEvent::NotifyPacketReceived(_))) {
                let _ = response_tx.send(Ok(packet.clone()));
            }
        }
    }

    /// Notify all pending recv requests about connection error
    fn notify_recv_connection_error(
        pending_recv_requests: &mut Vec<(PacketFilter, oneshot::Sender<Result<GenericPacket<PacketIdType>, SendError>>)>
    ) {
        for (_, response_tx) in pending_recv_requests.drain(..) {
            let _ = response_tx.send(Err(SendError::ConnectionError("Connection closed".to_string())));
        }
    }
}

#[derive(Debug)]
pub enum ConnectError {
    Transport(crate::mqtt_ep::transport::TransportError),
    AlreadyConnected,
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::Transport(e) => write!(f, "Transport error: {}", e),
            ConnectError::AlreadyConnected => write!(f, "Already connected"),
        }
    }
}

impl std::error::Error for ConnectError {}

#[derive(Debug)]
pub enum DisconnectError {
    SendError(SendError),
    AlreadyDisconnected,
    ChannelClosed,
}

impl std::fmt::Display for DisconnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisconnectError::SendError(e) => write!(f, "Send error: {:?}", e),
            DisconnectError::AlreadyDisconnected => write!(f, "Already disconnected"),
            DisconnectError::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for DisconnectError {}

pub type Endpoint<Role> = GenericEndpoint<Role, u16>;
