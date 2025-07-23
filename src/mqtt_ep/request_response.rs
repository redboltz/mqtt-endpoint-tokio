use crate::mqtt_ep::connection_option::ConnectionOption;
use crate::mqtt_ep::packet_filter::PacketFilter;
use mqtt_protocol_core::mqtt::connection::role::RoleType;
use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent, Sendable};
use mqtt_protocol_core::mqtt::packet::v5_0;
use mqtt_protocol_core::mqtt::packet::{GenericPacket, GenericStorePacket};
use mqtt_protocol_core::mqtt::types::IsPacketId;
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
use std::hash::Hash;
use tokio::sync::oneshot;

pub(crate) enum RequestResponse<Role, PacketIdType>
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
pub(crate) trait SendableErased<Role, PacketIdType>: Send
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
