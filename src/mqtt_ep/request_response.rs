use mqtt_protocol_core::mqtt::common::HashSet;

use crate::mqtt_ep::connection_error::ConnectionError;
use crate::mqtt_ep::packet::v5_0::GenericPublish;
use crate::mqtt_ep::packet::{GenericPacket, GenericStorePacket, IsPacketId};
use crate::mqtt_ep::transport::TransportOps;
use crate::mqtt_ep::Version;

use crate::mqtt_ep::connection_option::GenericConnectionOption;
use crate::mqtt_ep::endpoint::Mode;
use crate::mqtt_ep::packet_filter::PacketFilter;

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
use tokio::sync::oneshot;

pub(crate) enum RequestResponse<PacketIdType>
where
    PacketIdType: IsPacketId + Send + Sync,
{
    Send {
        packet: Box<GenericPacket<PacketIdType>>,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    Recv {
        filter: PacketFilter,
        response_tx: oneshot::Sender<Result<GenericPacket<PacketIdType>, ConnectionError>>,
    },
    AcquirePacketId {
        response_tx: oneshot::Sender<Result<PacketIdType, ConnectionError>>,
    },
    AcquirePacketIdWhenAvailable {
        response_tx: oneshot::Sender<Result<PacketIdType, ConnectionError>>,
    },
    RegisterPacketId {
        packet_id: PacketIdType,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    ReleasePacketId {
        packet_id: PacketIdType,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    Close {
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    Attach {
        transport: Box<dyn TransportOps + Send>,
        mode: Mode,
        options: GenericConnectionOption<PacketIdType>,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    GetStoredPackets {
        response_tx:
            oneshot::Sender<Result<Vec<GenericStorePacket<PacketIdType>>, ConnectionError>>,
    },
    SetOfflinePublish {
        offline_publish: bool,
        response_tx: oneshot::Sender<Result<(), ConnectionError>>,
    },
    GetQos2PublishHandled {
        response_tx: oneshot::Sender<Result<HashSet<PacketIdType>, ConnectionError>>,
    },
    GetReceiveMaximumVacancyForSend {
        response_tx: oneshot::Sender<Result<Option<u16>, ConnectionError>>,
    },
    GetProtocolVersion {
        response_tx: oneshot::Sender<Result<Version, ConnectionError>>,
    },
    IsPublishProcessing {
        packet_id: PacketIdType,
        response_tx: oneshot::Sender<Result<bool, ConnectionError>>,
    },
    RegulateForStore {
        packet: GenericPublish<PacketIdType>,
        response_tx: oneshot::Sender<Result<GenericPublish<PacketIdType>, ConnectionError>>,
    },
}
