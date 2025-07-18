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
    tx_send: mpsc::UnboundedSender<SendRequest<Role, PacketIdType>>,
    _marker: PhantomData<Role>,
}

enum SendRequest<Role, PacketIdType>
where
    Role: RoleType,
    PacketIdType: IsPacketId + Eq + Hash + Serialize + Send + Sync + 'static,
{
    Sendable {
        packet: Box<dyn SendableErased<Role, PacketIdType>>,
        response_tx: oneshot::Sender<Result<Vec<GenericEvent<PacketIdType>>, SendError>>,
    },
    GenericPacket {
        packet: GenericPacket<PacketIdType>,
        response_tx: oneshot::Sender<Result<Vec<GenericEvent<PacketIdType>>, SendError>>,
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
                    SendRequest::Sendable { packet, response_tx } => {
                        let events = packet.dispatch_send_boxed(&mut connection);
                        let _ = response_tx.send(Ok(events));
                    }
                    SendRequest::GenericPacket { packet: _packet, response_tx } => {
                        // TODO: GenericPacket doesn't implement Sendable trait
                        // Need to use send_generic_packet or find alternative approach
                        let events = vec![]; // Temporary placeholder
                        let _ = response_tx.send(Ok(events));
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
    /// This method accepts any packet type that implements `Sendable<Role, PacketIdType>`,
    /// providing compile-time verification that the packet is valid for the endpoint's role.
    pub async fn send<T>(&self, packet: T) -> Result<Vec<GenericEvent<PacketIdType>>, SendError>
    where
        T: Sendable<Role, PacketIdType> + Send + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(SendRequest::Sendable {
                packet: Box::new(packet),
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

    /// Send GenericPacket (for dynamic cases)
    ///
    /// This method accepts GenericPacket for cases where the packet type
    /// cannot be determined at compile time.
    pub async fn send_generic(&self, packet: GenericPacket<PacketIdType>) -> Result<Vec<GenericEvent<PacketIdType>>, SendError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.tx_send
            .send(SendRequest::GenericPacket {
                packet,
                response_tx,
            })
            .map_err(|_| SendError::ChannelClosed)?;

        response_rx
            .await
            .map_err(|_| SendError::ChannelClosed)?
    }

}

pub type Endpoint<Role> = GenericEndpoint<Role, u16>;