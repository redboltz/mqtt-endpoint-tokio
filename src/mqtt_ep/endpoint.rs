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
use tokio::sync::{mpsc, oneshot};
use tokio::io::{AsyncRead, AsyncWrite};
use serde::Serialize;

use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent, Sendable};
use mqtt_protocol_core::mqtt::connection::role::RoleType;
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
            let _stream = stream;
            
            while let Some(request) = rx_send.recv().await {
                match request {
                    RequestResponse::Sendable { packet, response_tx } => {
                        let events = packet.dispatch_send_boxed(&mut connection);
                        let _ = response_tx.send(Ok(events));
                    }
                    RequestResponse::AcquirePacketId { response_tx } => {
                        match connection.acquire_unique_packet_id() {
                            Ok(packet_id) => {
                                let _ = response_tx.send(Ok(packet_id));
                            }
                            Err(e) => {
                                let _ = response_tx.send(Err(SendError::ConnectionError(e.to_string())));
                            }
                        }
                    }
                    RequestResponse::RegisterPacketId { packet_id, response_tx } => {
                        match connection.register_packet_id(packet_id) {
                            Ok(()) => {
                                let _ = response_tx.send(Ok(()));
                            }
                            Err(e) => {
                                let _ = response_tx.send(Err(SendError::ConnectionError(e.to_string())));
                            }
                        }
                    }
                    RequestResponse::ReleasePacketId { packet_id, response_tx } => {
                        let _events = connection.release_packet_id(packet_id);
                        let _ = response_tx.send(Ok(()));
                    }
                }
            }
        });

        Self {
            tx_send,
            _marker: PhantomData,
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