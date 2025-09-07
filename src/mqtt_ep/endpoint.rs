// MIT License
//
// Copyright (c) 2025 Takatoshi Kondo
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::future;
use std::marker::PhantomData;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;

use crate::mqtt_ep::common::HashSet;
use crate::mqtt_ep::packet::v5_0::GenericPublish;
use crate::mqtt_ep::packet::{GenericPacket, GenericStorePacket, IsPacketId};
use crate::mqtt_ep::role::RoleType;
use crate::mqtt_ep::transport::{TransportError, TransportOps};
use crate::mqtt_ep::Version;

// Internal mqtt-protocol-core imports
use mqtt_protocol_core::mqtt;
use mqtt_protocol_core::mqtt::prelude::*;

use crate::mqtt_ep::connection_error::ConnectionError;
use crate::mqtt_ep::connection_option::GenericConnectionOption;
use crate::mqtt_ep::packet_filter::PacketFilter;
use crate::mqtt_ep::request_response::RequestResponse;

/// Connection mode for the attach operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Client mode - expects to receive CONNACK packet
    Client,
    /// Server mode - expects to receive CONNECT packet
    Server,
}

/// Builder for creating GenericEndpoint with custom configuration
pub struct GenericEndpointBuilder<Role, PacketIdType>
where
    Role: RoleType + Send + Sync,
    PacketIdType: IsPacketId + Send + Sync,
{
    version: Version,
    _marker: PhantomData<(Role, PacketIdType)>,
}

impl<Role, PacketIdType> GenericEndpointBuilder<Role, PacketIdType>
where
    Role: RoleType + Send + Sync,
    PacketIdType: IsPacketId + Send + Sync,
    <PacketIdType as IsPacketId>::Buffer: Send,
{
    /// Create a new builder with the specified MQTT version
    ///
    /// # Arguments
    ///
    /// * `version` - The MQTT protocol version to use (V3_1_1 or V5_0)
    ///
    /// # Returns
    ///
    /// A new `GenericEndpointBuilder` instance configured with the specified version
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let builder = mqtt_ep::endpoint::GenericEndpointBuilder::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// ```
    pub fn new(version: Version) -> Self {
        Self {
            version,
            _marker: PhantomData,
        }
    }

    /// Build the endpoint (initially in disconnected state)
    ///
    /// Creates a new endpoint instance with the configured settings. The endpoint starts
    /// in a disconnected state and must be connected to a transport before use.
    ///
    /// # Returns
    ///
    /// A new `GenericEndpoint` instance ready for connection
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpointBuilder::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0)
    ///     .build();
    /// ```
    pub fn build(self) -> GenericEndpoint<Role, PacketIdType> {
        GenericEndpoint::<Role, PacketIdType>::new(self.version)
    }
}

pub struct GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync,
    PacketIdType: IsPacketId + Send + Sync,
{
    #[allow(dead_code)] // Used in constructor and may be used for version checking in future
    version: Version,
    tx_send: mpsc::UnboundedSender<RequestResponse<PacketIdType>>,
    #[allow(dead_code)] // May be used for cleanup in future Drop implementation
    event_loop_handle: tokio::task::JoinHandle<()>,
    _marker: PhantomData<Role>,
}

// Type alias for complex type
type RecvRequestVec<PacketIdType> = Vec<(
    PacketFilter,
    oneshot::Sender<Result<GenericPacket<PacketIdType>, ConnectionError>>,
)>;

// Type alias for packet queue
type PacketQueueVec<PacketIdType> = Vec<(
    Box<GenericPacket<PacketIdType>>,
    oneshot::Sender<Result<(), ConnectionError>>,
)>;

// Type aliases for timer ref tuples
type TimerTupleRef<'a> = (
    &'a mut Option<tokio::task::JoinHandle<()>>,
    &'a mut Option<tokio::task::JoinHandle<()>>,
    &'a mut Option<tokio::task::JoinHandle<()>>,
);

// Type aliases for other complex tuples
type PacketIdRequestVec<PacketIdType> = Vec<oneshot::Sender<Result<PacketIdType, ConnectionError>>>;

type PendingRequestsTuple<'a, PacketIdType> = (
    &'a mut PacketIdRequestVec<PacketIdType>,
    &'a mut RecvRequestVec<PacketIdType>,
);

type ContextTuple<'a, PacketIdType> = (
    &'a mut PacketIdRequestVec<PacketIdType>,
    &'a mut Option<tokio::task::JoinHandle<()>>,
    &'a mut Option<Mode>,
    Duration,
);

impl<Role, PacketIdType> GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync,
    PacketIdType: IsPacketId + Send + Sync,
    <PacketIdType as IsPacketId>::Buffer: Send,
{
    /// Create a new builder for configuring the endpoint
    ///
    /// This method provides a builder pattern for constructing endpoints with custom
    /// configuration options. Use this when you need to set specific configuration
    /// parameters before creating the endpoint.
    ///
    /// # Arguments
    ///
    /// * `version` - The MQTT protocol version to use (V3_1_1 or V5_0)
    ///
    /// # Returns
    ///
    /// A new `GenericEndpointBuilder` instance for configuring the endpoint
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::builder(mqtt_ep::Version::V5_0)
    ///     .build();
    /// ```
    pub fn builder(version: Version) -> GenericEndpointBuilder<Role, PacketIdType> {
        GenericEndpointBuilder::new(version)
    }

    /// Create a new endpoint with default configuration (initially disconnected)
    ///
    /// This is a convenience method that creates an endpoint with default settings.
    /// The endpoint starts in a disconnected state and must be connected to a transport
    /// before it can send or receive MQTT packets.
    ///
    /// # Arguments
    ///
    /// * `version` - The MQTT protocol version to use (V3_1_1 or V5_0)
    ///
    /// # Returns
    ///
    /// A new `GenericEndpoint` instance with default configuration
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// ```
    pub fn new(version: Version) -> Self {
        let connection = mqtt::GenericConnection::new(version);
        let (tx_send, rx_send) = mpsc::unbounded_channel();

        // Start event loop immediately
        let event_loop_handle = tokio::spawn(Self::request_event_loop(connection, rx_send));

        Self {
            version,
            tx_send,
            event_loop_handle,
            _marker: PhantomData,
        }
    }

    /// Attach a transport with default connection options
    ///
    /// This method attaches an already connected and configured transport with default
    /// connection options. The transport should be fully established and ready for MQTT communication.
    /// Use `connect_helper` functions to establish connections before calling this method.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The transport type that implements `TransportOps + Send + 'static`
    ///
    /// # Arguments
    ///
    /// * `transport` - The already connected transport implementation
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the transport was attached successfully
    /// * `Err(ConnectionError)` - If the attachment failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - Transport configuration failure
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::connect_helper;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// let tcp_stream = connect_helper::connect_tcp("127.0.0.1:1883", None).await?;
    /// let transport = TcpTransport::from_stream(tcp_stream);
    /// endpoint.attach(transport, mqtt_ep::Mode::Client).await?;
    /// ```
    pub async fn attach<T>(&self, transport: T, mode: Mode) -> Result<(), ConnectionError>
    where
        T: TransportOps + Send + 'static,
    {
        self.attach_with_options(
            transport,
            mode,
            GenericConnectionOption::<PacketIdType>::default(),
        )
        .await
    }

    /// Attach a transport with specific connection options
    ///
    /// This method attaches an already connected and configured transport with custom
    /// connection options. The transport should be fully established and ready for MQTT communication.
    /// Use `connect_helper` functions to establish connections before calling this method.
    ///
    /// Custom connection options allow fine-grained control over connection behavior,
    /// such as session persistence, keep-alive settings, and protocol-specific features.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The transport type that implements `TransportOps + Send + 'static`
    ///
    /// # Arguments
    ///
    /// * `transport` - The already connected transport implementation
    /// * `options` - Custom connection options for behavior configuration
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the transport was attached successfully
    /// * `Err(ConnectionError)` - If the attachment failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - Transport configuration failure
    /// - Invalid connection options
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    /// use mqtt_endpoint_tokio::mqtt_ep::transport::connect_helper;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// let options = mqtt_ep::connection_option::GenericConnectionOption::<u16>::default();
    /// let tls_stream = connect_helper::connect_tcp_tls("127.0.0.1:8883", "localhost", None, None).await?;
    /// let transport = TlsTransport::from_stream(tls_stream);
    /// endpoint.attach_with_options(transport, mqtt_ep::Mode::Client, options).await?;
    /// ```
    pub async fn attach_with_options<T>(
        &self,
        transport: T,
        mode: Mode,
        options: GenericConnectionOption<PacketIdType>,
    ) -> Result<(), ConnectionError>
    where
        T: TransportOps + Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx_send
            .send(RequestResponse::Attach {
                transport: Box::new(transport),
                mode,
                options,
                response_tx,
            })
            .is_err()
        {
            return Err(ConnectionError::Transport(TransportError::NotConnected));
        }

        response_rx
            .await
            .map_err(|_| ConnectionError::Transport(TransportError::NotConnected))?
    }

    /// Close the current connection
    ///
    /// This method gracefully closes the current transport connection and cleans up
    /// associated resources. It will wait for the close operation to complete before
    /// returning. Multiple concurrent close requests are handled safely.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the connection was successfully closed
    /// * `Err(ConnectionError)` - If closing failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect and use endpoint ...
    /// endpoint.close().await?;
    /// ```
    pub async fn close(&self) -> Result<(), ConnectionError> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx_send
            .send(RequestResponse::Close { response_tx })
            .is_err()
        {
            return Err(ConnectionError::ChannelClosed);
        }

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Send MQTT packet with compile-time type safety
    ///
    /// This method accepts any packet type that implements `mqtt::connection::Sendable<Role, PacketIdType>`
    /// for compile-time verification, or `GenericPacket<PacketIdType>` for dynamic cases.
    /// All packets are converted to GenericPacket internally via the Into trait.
    ///
    /// The method ensures compile-time type safety by verifying that the packet type is valid
    /// for the current role (Client or Server). It will block until the packet is sent or
    /// an error occurs.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The packet type that implements Sendable for the current role and packet ID type
    ///
    /// # Arguments
    ///
    /// * `packet` - The MQTT packet to send
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the packet was successfully sent
    /// * `Err(ConnectionError)` - If sending failed due to connection issues or protocol errors
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint is not connected to a transport
    /// - The transport connection is closed
    /// - MQTT protocol violations
    /// - Network transmission errors
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let connect_packet = mqtt_ep::packet::v5_0::Connect::new("client_id");
    /// endpoint.send(connect_packet).await?;
    /// ```
    pub async fn send<T>(&self, packet: T) -> Result<(), ConnectionError>
    where
        T: Into<GenericPacket<PacketIdType>>
            + mqtt::connection::Sendable<Role, PacketIdType>
            + Send
            + 'static,
    {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::Send {
                packet: Box::<GenericPacket<PacketIdType>>::new(packet.into()),
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Receive any MQTT packet
    ///
    /// This method blocks until an MQTT packet is received from the transport.
    /// It accepts any packet type without filtering, making it suitable for
    /// general packet processing scenarios.
    ///
    /// # Returns
    ///
    /// * `Ok(GenericPacket<PacketIdType>)` - The received MQTT packet
    /// * `Err(ConnectionError)` - If receiving failed due to connection issues
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint is not connected to a transport
    /// - The transport connection is closed
    /// - Network reception errors
    /// - MQTT protocol parsing errors
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let packet = endpoint.recv().await?;
    /// println!("Received packet: {packet:?}");
    /// ```
    pub async fn recv(&self) -> Result<GenericPacket<PacketIdType>, ConnectionError> {
        self.recv_filtered(PacketFilter::Any).await
    }

    /// Receive MQTT packet matching the specified filter
    ///
    /// This method blocks until an MQTT packet matching the specified filter is received
    /// from the transport. Packets that don't match the filter are discarded, allowing
    /// for selective packet processing.
    ///
    /// # Arguments
    ///
    /// * `filter` - The packet filter criteria to match against incoming packets
    ///
    /// # Returns
    ///
    /// * `Ok(GenericPacket<PacketIdType>)` - The received MQTT packet matching the filter
    /// * `Err(ConnectionError)` - If receiving failed due to connection issues
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint is not connected to a transport
    /// - The transport connection is closed
    /// - Network reception errors
    /// - MQTT protocol parsing errors
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let connack = endpoint.recv_filtered(mqtt_ep::packet_filter::PacketFilter::Connack).await?;
    /// println!("Received CONNACK: {connack:?}");
    /// ```
    pub async fn recv_filtered(
        &self,
        filter: PacketFilter,
    ) -> Result<GenericPacket<PacketIdType>, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::Recv {
                filter,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Acquire a unique packet ID
    ///
    /// This method attempts to acquire a unique packet ID from the connection's packet ID manager.
    /// Packet IDs are required for QoS 1 and QoS 2 PUBLISH, SUBSCRIBE, and UNSUBSCRIBE packets.
    /// The method returns immediately with an error if no packet IDs are available.
    ///
    /// # Returns
    ///
    /// * `Ok(PacketIdType)` - A unique packet ID that can be used for MQTT packets requiring one
    /// * `Err(ConnectionError)` - If no packet IDs are available or connection issues occur
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - All packet IDs are currently in use
    /// - The endpoint's internal channel is closed
    /// - MQTT protocol violations
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let packet_id = endpoint.acquire_packet_id().await?;
    /// println!("Acquired packet ID: {packet_id}");
    /// ```
    pub async fn acquire_packet_id(&self) -> Result<PacketIdType, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::AcquirePacketId { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Acquire a unique packet ID, waiting until one becomes available
    ///
    /// Unlike acquire_packet_id(), this method will not return an error
    /// if all packet IDs are currently in use. Instead, it will wait until
    /// a packet ID is released and becomes available. This is useful when
    /// you need a packet ID but can tolerate waiting for one to become free.
    ///
    /// # Returns
    ///
    /// * `Ok(PacketIdType)` - A unique packet ID when one becomes available
    /// * `Err(ConnectionError)` - If connection issues occur (never due to packet ID exhaustion)
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    /// - MQTT protocol violations (not packet ID exhaustion)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let packet_id = endpoint.acquire_packet_id_when_available().await?;
    /// println!("Acquired packet ID (waited): {packet_id}");
    /// ```
    pub async fn acquire_packet_id_when_available(&self) -> Result<PacketIdType, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::AcquirePacketIdWhenAvailable { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Register a packet ID as in use
    ///
    /// This method manually registers a packet ID as being in use by the connection.
    /// This is typically used when restoring session state or when managing packet IDs
    /// externally. Once registered, the packet ID will not be returned by acquire_packet_id()
    /// until it is explicitly released.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet ID to register as in use
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the packet ID was successfully registered
    /// * `Err(ConnectionError)` - If registration failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    /// - The packet ID is already in use
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// endpoint.register_packet_id(100).await?;
    /// ```
    pub async fn register_packet_id(&self, packet_id: PacketIdType) -> Result<(), ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::RegisterPacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Release a packet ID
    ///
    /// This method releases a previously acquired or registered packet ID, making it
    /// available for reuse. This should be called when a packet exchange is complete
    /// (e.g., after receiving PUBACK for a QoS 1 PUBLISH, or PUBCOMP for a QoS 2 PUBLISH).
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet ID to release
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the packet ID was successfully released
    /// * `Err(ConnectionError)` - If release failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    /// - The packet ID was not in use
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// let packet_id = endpoint.acquire_packet_id().await?;
    /// // ... use packet_id for MQTT packet ...
    /// endpoint.release_packet_id(packet_id).await?;
    /// ```
    pub async fn release_packet_id(&self, packet_id: PacketIdType) -> Result<(), ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::ReleasePacketId {
                packet_id,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Get all stored packets from the connection store
    ///
    /// This method retrieves all currently stored packets from the connection.
    /// This is typically used for session persistence to save in-flight QoS 1 and QoS 2 packets
    /// before closing the connection, so they can be restored later with restore_packets().
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<GenericStorePacket<PacketIdType>>)` - All currently stored packets
    /// * `Err(ConnectionError)` - If retrieval failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect and send some packets ...
    /// let stored_packets = endpoint.get_stored_packets().await?;
    /// println!("Found {} stored packets", stored_packets.len());
    /// ```
    pub async fn get_stored_packets(
        &self,
    ) -> Result<Vec<GenericStorePacket<PacketIdType>>, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::GetStoredPackets { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Set offline publish state
    ///
    /// This method controls whether the endpoint should handle publish packets when offline.
    /// When set to true, publish packets may be queued or handled differently when the transport is disconnected.
    ///
    /// # Arguments
    ///
    /// * `offline_publish` - Whether to enable offline publish handling
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the setting was successfully applied
    /// * `Err(ConnectionError)` - If setting failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// endpoint.set_offline_publish(true).await?;
    /// ```
    pub async fn set_offline_publish(&self, offline_publish: bool) -> Result<(), ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::SetOfflinePublish {
                offline_publish,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Get QoS 2 publish handled packet IDs
    ///
    /// This method retrieves all packet IDs for QoS 2 PUBLISH packets that have been handled.
    /// This is typically used for session persistence to save which QoS 2 publishes have been processed
    /// before closing the connection, so duplicate handling can be avoided when restoring the session.
    ///
    /// # Returns
    ///
    /// * `Ok(mqtt_ep::common::HashSet<PacketIdType>)` - Set of packet IDs for handled QoS 2 PUBLISH packets
    /// * `Err(ConnectionError)` - If retrieval failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... process some QoS 2 publishes ...
    /// let handled_pids = endpoint.get_qos2_publish_handled_pids().await?;
    /// println!("Handled {} QoS 2 publishes", handled_pids.len());
    /// ```
    pub async fn get_qos2_publish_handled_pids(
        &self,
    ) -> Result<HashSet<PacketIdType>, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::GetQos2PublishHandled { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Get receive maximum vacancy for send
    ///
    /// This method returns the number of additional QoS 1 and QoS 2 packets that can be sent
    /// before reaching the receive maximum limit imposed by the peer. This helps implement
    /// flow control to avoid exceeding the peer's processing capacity.
    ///
    /// # Returns
    ///
    /// * `Ok(Some(u16))` - Number of additional packets that can be sent (MQTT v5.0)
    /// * `Ok(None)` - No receive maximum limit (MQTT v3.1.1 or unlimited)
    /// * `Err(ConnectionError)` - If retrieval failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// // ... connect to transport ...
    /// if let Some(vacancy) = endpoint.get_receive_maximum_vacancy_for_send().await? {
    ///     println!("Can send {vacancy} more QoS 1/2 packets");
    /// }
    /// ```
    pub async fn get_receive_maximum_vacancy_for_send(
        &self,
    ) -> Result<Option<u16>, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::GetReceiveMaximumVacancyForSend { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Get protocol version
    ///
    /// This method returns the MQTT protocol version currently being used by the connection.
    /// This is a memo (cached) value that reflects the protocol version negotiated during connection establishment.
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - The current MQTT protocol version (V3_1_1 or V5_0)
    /// * `Err(ConnectionError)` - If retrieval failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// let version = endpoint.get_protocol_version().await?;
    /// println!("Protocol version: {version:?}");
    /// ```
    pub async fn get_protocol_version(&self) -> Result<Version, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::GetProtocolVersion { response_tx })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Check if a publish packet is currently being processed
    ///
    /// This method checks whether a QoS 1 or QoS 2 PUBLISH packet with the given packet ID
    /// is currently being processed. This can be used to determine if a packet ID is in use
    /// for publish operations before attempting to use it for other packet types.
    ///
    /// # Arguments
    ///
    /// * `packet_id` - The packet ID to check
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - The packet ID is currently being used for a publish operation
    /// * `Ok(false)` - The packet ID is not being used for a publish operation
    /// * `Err(ConnectionError)` - If checking failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// let packet_id = 100;
    /// if endpoint.is_publish_processing(packet_id).await? {
    ///     println!("Packet ID {packet_id} is being used for publish");
    /// }
    /// ```
    pub async fn is_publish_processing(
        &self,
        packet_id: PacketIdType,
    ) -> Result<bool, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::IsPublishProcessing {
                packet_id,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Regulate a publish packet for store
    ///
    /// This method processes a PUBLISH packet to ensure it conforms to the connection's
    /// current state and regulations before storing it. This can include validation
    /// and transformation of the packet according to MQTT protocol rules.
    ///
    /// # Arguments
    ///
    /// * `packet` - The PUBLISH packet to regulate
    ///
    /// # Returns
    ///
    /// * `Ok(GenericPublish<PacketIdType>)` - The regulated PUBLISH packet
    /// * `Err(ConnectionError)` - If regulation failed
    ///
    /// # Errors
    ///
    /// This method can return errors in the following cases:
    /// - The endpoint's internal channel is closed
    /// - MQTT protocol violations in the packet
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use mqtt_endpoint_tokio::mqtt_ep;
    /// use mqtt_endpoint_tokio::mqtt_ep::prelude::*;
    ///
    /// let endpoint = mqtt_ep::endpoint::GenericEndpoint::<mqtt_ep::role::Client, u16>::new(mqtt_ep::Version::V5_0);
    /// let publish_packet = mqtt_ep::packet::v5_0::GenericPublish::new("topic", "payload");
    /// let regulated = endpoint.regulate_for_store(publish_packet).await?;
    /// ```
    pub async fn regulate_for_store(
        &self,
        packet: GenericPublish<PacketIdType>,
    ) -> Result<GenericPublish<PacketIdType>, ConnectionError> {
        let tx_send = self.get_tx_send();
        let (response_tx, response_rx) = oneshot::channel();

        tx_send
            .send(RequestResponse::RegulateForStore {
                packet,
                response_tx,
            })
            .map_err(|_| ConnectionError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| ConnectionError::ChannelClosed)?
    }

    /// Get the tx_send channel - always available in new architecture
    fn get_tx_send(&self) -> mpsc::UnboundedSender<RequestResponse<PacketIdType>> {
        self.tx_send.clone()
    }

    /// Apply connection options to the MQTT connection
    fn apply_connection_options(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        options: GenericConnectionOption<PacketIdType>,
    ) -> (Duration, Option<usize>, bool) {
        // Apply scalar settings (these are Copy, so we can access them after partial move)
        connection.set_pingreq_send_interval(*options.pingreq_send_interval_ms());
        connection.set_auto_pub_response(*options.auto_pub_response());
        connection.set_auto_ping_response(*options.auto_ping_response());
        connection.set_auto_map_topic_alias_send(*options.auto_map_topic_alias_send());
        connection.set_auto_replace_topic_alias_send(*options.auto_replace_topic_alias_send());
        connection.set_pingresp_recv_timeout(*options.pingresp_recv_timeout_ms());

        // Get shutdown timeout and optional recv buffer size before moving options
        let shutdown_timeout = Duration::from_millis(*options.shutdown_timeout_ms());
        let recv_buffer_size = *options.recv_buffer_size();

        let queuing_receive_maximum = *options.queuing_receive_maximum();

        // Move large collections efficiently using the helper method
        let (restore_packets, restore_qos2_publish_handled) = options.into_restore_data();
        connection.restore_packets(restore_packets);
        connection.restore_qos2_publish_handled(restore_qos2_publish_handled);

        (shutdown_timeout, recv_buffer_size, queuing_receive_maximum)
    }

    /// Event loop that handles transport I/O and MQTT protocol logic
    async fn request_event_loop(
        mut connection: mqtt::GenericConnection<Role, PacketIdType>,
        mut rx_send: mpsc::UnboundedReceiver<RequestResponse<PacketIdType>>,
    ) {
        let mut pingreq_send_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut pingreq_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut pingresp_recv_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut connection_establish_timer: Option<tokio::task::JoinHandle<()>> = None;
        let mut connection_mode: Option<Mode> = None;
        let (timer_tx, mut timer_rx) = mpsc::unbounded_channel::<mqtt::connection::TimerKind>();
        let (connection_timeout_tx, mut connection_timeout_rx) = mpsc::unbounded_channel::<()>();

        let mut pending_packet_id_requests: Vec<
            oneshot::Sender<Result<PacketIdType, ConnectionError>>,
        > = Vec::new();
        let mut pending_close_notifications: Vec<oneshot::Sender<Result<(), ConnectionError>>> =
            Vec::new();
        let mut pending_recv_requests: RecvRequestVec<PacketIdType> = Vec::new();
        let mut transport: Option<Box<dyn TransportOps + Send>> = None;
        let mut recv_buffer_size = 4096usize; // Default, will be updated when transport is attached
        let mut shutdown_timeout = Duration::from_secs(5); // Default, will be updated when transport is attached
        let mut read_buffer = vec![0u8; recv_buffer_size];
        let mut buffer_size: usize = 0; // Actual data size in read_buffer
        let mut consumed_bytes: usize = 0; // Bytes consumed by connection.recv()
        let mut queuing_receive_maximum = false;
        let mut packet_queue: PacketQueueVec<PacketIdType> = Vec::new();

        loop {
            tokio::select! {
                // Handle requests from external API
                request = rx_send.recv() => {
                    match request {
                        Some(RequestResponse::Send { packet, response_tx }) => {
                            if let Some(ref mut t) = transport {
                                if queuing_receive_maximum && connection.get_receive_maximum_vacancy_for_send().map_or(false, |v| v == 0) {
                                    // Add packet to queue for later retry
                                    packet_queue.push((packet, response_tx));
                                } else {
                                    let events = connection.send(*packet);
                                    match Self::process_connection_events(
                                        &mut connection,
                                        t,
                                        (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                        &timer_tx,
                                        &mut pending_packet_id_requests,
                                        shutdown_timeout,
                                        events,
                                        &mut packet_queue
                                    )
                                    .await {
                                        Ok(()) => {
                                            let _ = response_tx.send(Ok(()));
                                        }
                                        Err(error) => {
                                            let _ = response_tx.send(Err(error));
                                        }
                                    }
                                }
                            } else {
                                let _ = response_tx.send(Err(ConnectionError::NotConnected));
                            }
                        }
                        Some(RequestResponse::Recv { filter, response_tx }) => {
                            // Add to pending queue (non-blocking)
                            if transport.is_some() {
                                pending_recv_requests.push((filter, response_tx));
                            } else {
                                let _ = response_tx.send(Err(ConnectionError::NotConnected));
                            }
                        }
                        Some(RequestResponse::AcquirePacketId { response_tx }) => {
                            match connection.acquire_packet_id() {
                                Ok(packet_id) => {
                                    let _ = response_tx.send(Ok(packet_id));
                                }
                                Err(e) => {
                                    let _ = response_tx.send(Err(ConnectionError::Mqtt(e)));
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
                            let events = connection.release_packet_id(packet_id);
                            if let Some(ref mut t) = transport {
                                match Self::process_connection_events(
                                    &mut connection,
                                    t,
                                    (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                    &timer_tx,
                                    &mut pending_packet_id_requests,
                                    shutdown_timeout,
                                    events,
                                    &mut packet_queue
                                ).await {
                                    Ok(()) => {
                                        let _ = response_tx.send(Ok(()));
                                    }
                                    Err(error) => {
                                        let _ = response_tx.send(Err(error));
                                    }
                                }
                            } else {
                                let _ = response_tx.send(Ok(())); // Release packet ID doesn't require transport
                            }
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
                        Some(RequestResponse::Attach { transport: new_transport, mode, options, response_tx }) => {
                            // Get timeout value before options is consumed
                            let timeout_ms = *options.connection_establish_timeout_ms();

                            let (new_shutdown_timeout, new_recv_buffer_size, new_queuing_receive_maximum) = Self::apply_connection_options(&mut connection, options);
                            shutdown_timeout = new_shutdown_timeout;
                            queuing_receive_maximum = new_queuing_receive_maximum;

                            // Update recv buffer size if specified
                            if let Some(new_size) = new_recv_buffer_size {
                                if new_size != recv_buffer_size {
                                    recv_buffer_size = new_size;
                                    read_buffer = vec![0u8; recv_buffer_size];
                                    buffer_size = 0;
                                    consumed_bytes = 0;
                                }
                            }

                            // Set up connection establish timeout if specified
                            if timeout_ms > 0 {
                                connection_mode = Some(mode);
                                let timeout_tx_clone = connection_timeout_tx.clone();
                                connection_establish_timer = Some(tokio::spawn(async move {
                                    sleep(Duration::from_millis(timeout_ms)).await;
                                    let _ = timeout_tx_clone.send(());
                                }));
                            }

                            transport = Some(new_transport);
                            let _ = response_tx.send(Ok(()));
                        }
                        Some(RequestResponse::Close { response_tx }) => {
                            // Handle close request
                            Self::handle_close_request(
                                &mut connection,
                                &mut transport,
                                &mut pending_close_notifications,
                                (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                &mut pending_packet_id_requests,
                                &mut packet_queue,
                                response_tx
                            ).await;
                        }
                        Some(RequestResponse::SetOfflinePublish { offline_publish, response_tx }) => {
                            connection.set_offline_publish(offline_publish);
                            let _ = response_tx.send(Ok(()));
                        }
                        Some(RequestResponse::GetQos2PublishHandled { response_tx }) => {
                            let pids = connection.get_qos2_publish_handled();
                            let _ = response_tx.send(Ok(pids));
                        }
                        Some(RequestResponse::GetReceiveMaximumVacancyForSend { response_tx }) => {
                            let vacancy = connection.get_receive_maximum_vacancy_for_send();
                            let _ = response_tx.send(Ok(vacancy));
                        }
                        Some(RequestResponse::GetProtocolVersion { response_tx }) => {
                            let version = connection.get_protocol_version();
                            let _ = response_tx.send(Ok(version));
                        }
                        Some(RequestResponse::IsPublishProcessing { packet_id, response_tx }) => {
                            let is_processing = connection.is_publish_processing(packet_id);
                            let _ = response_tx.send(Ok(is_processing));
                        }
                        None => break, // Channel closed
                    }
                }

                // Handle timer expiration
                timer_kind = timer_rx.recv() => {
                    if let Some(kind) = timer_kind {
                        match kind {
                            mqtt::connection::TimerKind::PingreqSend => pingreq_send_timer = None,
                            mqtt::connection::TimerKind::PingreqRecv => pingreq_recv_timer = None,
                            mqtt::connection::TimerKind::PingrespRecv => pingresp_recv_timer = None,
                        }
                        let events = connection.notify_timer_fired(kind);
                        if let Some(ref mut t) = transport {
                            let _ = Self::process_connection_events(
                                &mut connection,
                                t,
                                (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                &timer_tx,
                                &mut pending_packet_id_requests,
                                shutdown_timeout,
                                events,
                                &mut packet_queue
                            ).await;
                            // Note: We log MQTT errors from timer events but don't propagate them
                            // as there's no specific API call to respond to
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
                            // Check if we have unconsumed data in read_buffer
                            if consumed_bytes < buffer_size {
                                // Process remaining data in buffer
                                Some(Ok(0)) // Signal to process existing buffer
                            } else {
                                // Need new data from transport
                                Some(t.recv(&mut read_buffer).await)
                            }
                        }
                    } else {
                        // No transport available
                        future::pending().await
                    }
                } => {
                    if let Some(result) = recv_result {
                        match result {
                            Ok(n) if n > 0 => {
                                // New data received from transport
                                buffer_size = n;
                                consumed_bytes = 0;

                                // Process data in read_buffer
                                Self::process_read_buffer(
                                    &mut connection,
                                    (&mut read_buffer, &mut buffer_size, &mut consumed_bytes),
                                    &mut pending_recv_requests,
                                    &mut packet_queue,
                                    transport.as_mut().unwrap(),
                                    (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                    &timer_tx,
                                    (&mut pending_packet_id_requests, &mut connection_establish_timer, &mut connection_mode, shutdown_timeout),
                                ).await;
                            }
                            Ok(_) => {
                                // Process existing buffer data (n=0 case) or handle connection close
                                if consumed_bytes < buffer_size {
                                    // Process remaining data in buffer
                                    Self::process_read_buffer(
                                        &mut connection,
                                        (&mut read_buffer, &mut buffer_size, &mut consumed_bytes),
                                        &mut pending_recv_requests,
                                        &mut packet_queue,
                                        transport.as_mut().unwrap(),
                                        (&mut pingreq_send_timer, &mut pingreq_recv_timer, &mut pingresp_recv_timer),
                                        &timer_tx,
                                        (&mut pending_packet_id_requests, &mut connection_establish_timer, &mut connection_mode, shutdown_timeout),
                                    ).await;
                                } else {
                                    // Connection closed (n = 0) - notify all pending recv requests
                                    Self::notify_recv_connection_error(&mut pending_recv_requests);
                                    transport = None;
                                    buffer_size = 0;
                                    consumed_bytes = 0;
                                }
                            }
                            Err(_) => {
                                // Connection error - notify all pending recv requests
                                Self::notify_recv_connection_error(&mut pending_recv_requests);
                                transport = None;
                                buffer_size = 0;
                                consumed_bytes = 0;
                            }
                        }
                    }
                }

                // Handle connection establish timeout
                _ = connection_timeout_rx.recv() => {
                    // Connection establishment timeout occurred
                    if connection_establish_timer.is_some() {
                        connection_establish_timer = None;
                        connection_mode = None;

                        // Perform close equivalent processing
                        Self::notify_recv_connection_error(&mut pending_recv_requests);
                        if let Some(ref mut t) = transport {
                            t.shutdown(shutdown_timeout).await;
                        }
                        transport = None;
                        buffer_size = 0;
                        consumed_bytes = 0;
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
        if let Some(handle) = connection_establish_timer {
            handle.abort();
        }
    }

    /// Handle close request with proper queueing
    async fn handle_close_request(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        transport: &mut Option<Box<dyn TransportOps + Send>>,
        pending_close_notifications: &mut Vec<oneshot::Sender<Result<(), ConnectionError>>>,
        timers: TimerTupleRef<'_>,
        pending_packet_id_requests: &mut PacketIdRequestVec<PacketIdType>,
        packet_queue: &mut PacketQueueVec<PacketIdType>,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
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

        // Shutdown transport first
        let transport_ref = transport
            .as_mut()
            .expect("Transport should be Some when close is requested - this is a bug");
        let _ = transport_ref.shutdown(Duration::from_secs(5)).await; // Note: This uses default timeout since we don't have access to connection options here

        // Notify connection about close and process events (including timer cancellation)
        let events = connection.notify_closed();
        let transport_ref = transport
            .as_mut()
            .expect("Transport should still be Some after shutdown - this is a bug");
        let (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer) = timers;
        let _ = Self::process_connection_events(
            connection,
            transport_ref,
            (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer),
            &mpsc::unbounded_channel().0, // Dummy timer_tx since we're closing
            pending_packet_id_requests,
            Duration::from_secs(5), // Default timeout for close
            events,
            packet_queue,
        )
        .await;
        // Note: We ignore MQTT errors during close as the connection is being terminated

        // Clear transport after processing events
        *transport = None;

        // Notify all pending close requests
        for tx in pending_close_notifications.drain(..) {
            let _ = tx.send(Ok(()));
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_connection_events<S>(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        transport: &mut S,
        timers: TimerTupleRef<'_>,
        timer_tx: &mpsc::UnboundedSender<mqtt::connection::TimerKind>,
        pending_packet_id_requests: &mut PacketIdRequestVec<PacketIdType>,
        shutdown_timeout: Duration,
        events: Vec<mqtt::connection::GenericEvent<PacketIdType>>,
        packet_queue: &mut PacketQueueVec<PacketIdType>,
    ) -> Result<(), ConnectionError>
    where
        S: TransportOps,
    {
        let (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer) = timers;
        let mut first_error: Option<ConnectionError> = None;
        for event in events {
            match event {
                mqtt::connection::GenericEvent::NotifyPacketIdReleased(_packet_id) => {
                    Self::process_packet_id_waiting_queue(connection, pending_packet_id_requests);
                    // Process queued packets inline to avoid recursion
                    // We'll process them without complex error handling to avoid move issues
                    while !packet_queue.is_empty()
                        && connection
                            .get_receive_maximum_vacancy_for_send()
                            .map_or(false, |v| v > 0)
                    {
                        let (packet, response_tx) = packet_queue.remove(0);

                        // Simply try to send the packet without detailed error handling
                        let events = connection.send(*packet);
                        let mut sent = false;

                        for event in events {
                            if let mqtt::connection::GenericEvent::RequestSendPacket {
                                packet,
                                ..
                            } = event
                            {
                                let buffers = packet.to_buffers();
                                if transport.send(&buffers).await.is_ok() {
                                    sent = true;
                                }
                            }
                        }

                        // Simple success/failure response without detailed error propagation
                        if sent {
                            let _ = response_tx.send(Ok(()));
                        } else {
                            let _ = response_tx.send(Err(ConnectionError::NotConnected));
                        }
                    }
                }
                mqtt::connection::GenericEvent::RequestSendPacket {
                    packet,
                    release_packet_id_if_send_error,
                } => {
                    let buffers = packet.to_buffers();
                    let send_result = transport.send(&buffers).await;

                    match send_result {
                        Ok(_) => {
                            // Successfully sent - TransportOps::send handles flushing internally
                        }
                        Err(_) => {
                            if let Some(packet_id) = release_packet_id_if_send_error {
                                let release_events = connection.release_packet_id(packet_id);
                                let mut empty_queue = Vec::new();
                                let _ = Box::pin(Self::process_connection_events(
                                    connection,
                                    transport,
                                    (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer),
                                    timer_tx,
                                    &mut empty_queue,
                                    Duration::from_secs(5), // Default timeout for packet ID release
                                    release_events,
                                    packet_queue,
                                ))
                                .await;
                                // Note: We ignore MQTT errors during packet ID release
                                // as they are secondary to the send failure
                            }
                        }
                    }
                }
                mqtt::connection::GenericEvent::RequestTimerReset { kind, duration_ms } => {
                    match kind {
                        mqtt::connection::TimerKind::PingreqSend => {
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
                        mqtt::connection::TimerKind::PingreqRecv => {
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
                        mqtt::connection::TimerKind::PingrespRecv => {
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
                mqtt::connection::GenericEvent::RequestTimerCancel(kind) => match kind {
                    mqtt::connection::TimerKind::PingreqSend => {
                        if let Some(handle) = pingreq_send_timer.take() {
                            handle.abort();
                        }
                    }
                    mqtt::connection::TimerKind::PingreqRecv => {
                        if let Some(handle) = pingreq_recv_timer.take() {
                            handle.abort();
                        }
                    }
                    mqtt::connection::TimerKind::PingrespRecv => {
                        if let Some(handle) = pingresp_recv_timer.take() {
                            handle.abort();
                        }
                    }
                },
                mqtt::connection::GenericEvent::RequestClose => {
                    let _ = TransportOps::shutdown(transport, shutdown_timeout).await;
                }
                mqtt::connection::GenericEvent::NotifyError(error) => {
                    // Store the first MQTT protocol error for API response
                    // Only store the first error if none has been stored yet
                    if first_error.is_none() {
                        first_error = Some(ConnectionError::from(error));
                    }
                    // Also log for debugging purposes
                    eprintln!("MQTT protocol error: {error:?}");
                }
                mqtt::connection::GenericEvent::NotifyPacketReceived(_packet) => {
                    // This should not happen in process_events
                    // NotifyPacketReceived is handled separately in RequestResponse::Recv
                }
            }
        }

        // Return the first error if any occurred, otherwise Ok
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(())
        }
    }

    /// Process the packet ID waiting queue when a packet ID is released
    fn process_packet_id_waiting_queue(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        pending_requests: &mut Vec<oneshot::Sender<Result<PacketIdType, ConnectionError>>>,
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
                Err(mqtt_error) => {
                    // Failed to acquire packet ID - convert MqttError to ConnectionError
                    let connection_error = ConnectionError::from(mqtt_error);
                    // Send error to the first waiting requester and stop processing to maintain order
                    if !pending_requests.is_empty() {
                        // Remove the first pending request and send the error
                        let response_tx = pending_requests.remove(0);
                        let _ = response_tx.send(Err(connection_error));
                    }
                    break;
                }
            }
        }
    }

    /// Process all connection events first, then handle packet filtering for recv requests
    #[allow(clippy::too_many_arguments)]
    async fn process_received_packets_and_events<S>(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        transport: &mut S,
        timers: TimerTupleRef<'_>,
        timer_tx: &mpsc::UnboundedSender<mqtt::connection::TimerKind>,
        pending_requests: PendingRequestsTuple<'_, PacketIdType>,
        packet_queue: &mut PacketQueueVec<PacketIdType>,
        connection_ctx: (&mut Option<tokio::task::JoinHandle<()>>, &mut Option<Mode>),
        events_and_timeout: (Vec<mqtt::connection::GenericEvent<PacketIdType>>, Duration),
    ) where
        S: TransportOps,
    {
        let (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer) = timers;
        let (pending_packet_id_requests, pending_recv_requests) = pending_requests;
        let (connection_establish_timer, connection_mode) = connection_ctx;
        let (events, shutdown_timeout) = events_and_timeout;

        // First, process all connection events
        let _ = Self::process_connection_events(
            connection,
            transport,
            (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer),
            timer_tx,
            pending_packet_id_requests,
            shutdown_timeout,
            events.clone(),
            packet_queue,
        )
        .await;
        // Note: We log MQTT errors from received packets but don't propagate them
        // as they're not related to any specific API call but rather incoming data

        // Then, handle packet filtering for recv requests
        // Look for NotifyPacketReceived event (there should be 0 or 1)
        for event in &events {
            if let mqtt::connection::GenericEvent::NotifyPacketReceived(packet) = event {
                // Check if we should cancel connection establish timeout
                if let (Some(timer), Some(mode)) = (
                    connection_establish_timer.as_ref(),
                    connection_mode.as_ref(),
                ) {
                    use crate::mqtt_ep::packet::GenericPacket;
                    let should_cancel = matches!(
                        (mode, packet),
                        (Mode::Client, GenericPacket::V3_1_1Connack(_))
                            | (Mode::Client, GenericPacket::V5_0Connack(_))
                            | (Mode::Server, GenericPacket::V3_1_1Connect(_))
                            | (Mode::Server, GenericPacket::V5_0Connect(_))
                    );

                    if should_cancel {
                        timer.abort();
                        *connection_establish_timer = None;
                        *connection_mode = None;
                    }
                }

                // Process pending recv requests in FIFO order (from front)
                if let Some((filter, _)) = pending_recv_requests.first() {
                    if filter.matches(packet) {
                        // Remove the first (oldest) request and send response
                        let (_, response_tx) = pending_recv_requests.remove(0);
                        let _ = response_tx.send(Ok(packet.clone()));
                        break; // Only satisfy one request per packet
                    }
                    // If packet doesn't match the first filter, we don't consume any request
                    // The packet will be "lost" and the select loop will automatically
                    // call t.recv() again since pending_recv_requests is not empty
                }
                break; // Only process the first NotifyPacketReceived event
            }
        }
    }

    /// Notify all pending recv requests about connection error
    fn notify_recv_connection_error(pending_recv_requests: &mut RecvRequestVec<PacketIdType>) {
        for (_, response_tx) in pending_recv_requests.drain(..) {
            let _ = response_tx.send(Err(ConnectionError::NotConnected));
        }
    }

    /// Process read buffer data - handles one packet per call as per endpoint.recv() semantics
    #[allow(clippy::too_many_arguments)]
    async fn process_read_buffer<S>(
        connection: &mut mqtt::GenericConnection<Role, PacketIdType>,
        buffer_ctx: (&mut [u8], &mut usize, &mut usize),
        pending_recv_requests: &mut RecvRequestVec<PacketIdType>,
        packet_queue: &mut PacketQueueVec<PacketIdType>,
        transport: &mut S,
        timers: TimerTupleRef<'_>,
        timer_tx: &mpsc::UnboundedSender<mqtt::connection::TimerKind>,
        context: ContextTuple<'_, PacketIdType>,
    ) where
        S: TransportOps,
    {
        let (read_buffer, buffer_size, consumed_bytes) = buffer_ctx;
        let (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer) = timers;
        let (
            pending_packet_id_requests,
            connection_establish_timer,
            connection_mode,
            shutdown_timeout,
        ) = context;

        if *consumed_bytes >= *buffer_size {
            return; // No data to process
        }

        // Create cursor starting from unconsumed data
        let unconsumed_data = &read_buffer[*consumed_bytes..*buffer_size];
        let mut cursor = mqtt::common::Cursor::new(unconsumed_data);
        let events = connection.recv(&mut cursor);
        let position_after = cursor.position();

        // Update consumed bytes based on what connection.recv() processed
        *consumed_bytes += position_after as usize;

        // Process all connection events first, then handle packet filtering
        Self::process_received_packets_and_events(
            connection,
            transport,
            (pingreq_send_timer, pingreq_recv_timer, pingresp_recv_timer),
            timer_tx,
            (pending_packet_id_requests, pending_recv_requests),
            packet_queue,
            (connection_establish_timer, connection_mode),
            (events, shutdown_timeout),
        )
        .await;
    }
}

pub type Endpoint<Role> = GenericEndpoint<Role, u16>;
