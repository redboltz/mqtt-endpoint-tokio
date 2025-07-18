use std::marker::PhantomData;
use tokio::sync::{mpsc, oneshot};
use tokio::io::{AsyncRead, AsyncWrite};
use serde::Serialize;

use mqtt_protocol_core::mqtt::connection::{GenericConnection, GenericEvent};
use mqtt_protocol_core::mqtt::connection::role::RoleType;
use mqtt_protocol_core::mqtt::Version;
use mqtt_protocol_core::mqtt::packet::GenericPacket;
use mqtt_protocol_core::mqtt::types::IsPacketId;

pub struct GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Serialize + Send + Sync + 'static,
{
    tx_send: mpsc::UnboundedSender<SendRequest<PacketIdType>>,
    _marker: PhantomData<Role>,
}

enum SendRequest<PacketIdType>
where
    PacketIdType: IsPacketId + Serialize + Send + Sync + 'static,
{
    GenericPacket {
        packet: GenericPacket<PacketIdType>,
        response_tx: oneshot::Sender<Result<Vec<GenericEvent<PacketIdType>>, SendError>>,
    },
}

#[derive(Debug, Clone)]
pub enum SendError {
    ChannelClosed,
    ConnectionError(String),
}

impl<Role, PacketIdType> GenericEndpoint<Role, PacketIdType>
where
    Role: RoleType + Send + Sync + 'static,
    PacketIdType: IsPacketId + Serialize + Send + Sync + 'static,
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
            
            while let Some(request) = rx_send.recv().await {
                match request {
                    SendRequest::GenericPacket { packet, response_tx } => {
                        let events = vec![GenericEvent::RequestSendPacket {
                            packet,
                            release_packet_id_if_send_error: None,
                        }];
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

    pub async fn send(&self, packet: GenericPacket<PacketIdType>) -> Result<Vec<GenericEvent<PacketIdType>>, SendError> {
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