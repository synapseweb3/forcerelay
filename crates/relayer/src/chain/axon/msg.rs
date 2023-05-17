use std::str::FromStr;

use ethers::types::Bytes;
use ibc_proto::google::protobuf::Any;
use ibc_relayer_types::{
    core::{
        ics03_connection::{
            self,
            connection::Counterparty,
            msgs::{
                conn_open_init::{self, MsgConnectionOpenInit},
                conn_open_try::{self, MsgConnectionOpenTry},
            },
        },
        ics04_channel::{packet::Packet, timeout::TimeoutHeight},
        ics23_commitment::commitment::{CommitmentPrefix, CommitmentProofBytes},
        ics24_host::identifier::{ClientId, ConnectionId},
    },
    events::IbcEvent,
    proofs::Proofs,
    timestamp::Timestamp,
    tx_msg::Msg,
    Height,
};

use super::contract;
use crate::{error::Error, object};

fn into_ethers_client_id(value: Option<ClientId>) -> String {
    match value {
        Some(v) => v.as_str().into(),
        None => String::from(""),
    }
}

fn into_ethers_client_state(value: Option<Any>) -> Bytes {
    match value {
        Some(v) => v.value.into(),
        None => Bytes::new(),
    }
}

fn into_ethers_connection_id(value: Option<ConnectionId>) -> String {
    match value {
        Some(v) => v.as_str().into(),
        None => String::from(""),
    }
}

fn into_ethers_proofs(
    value: Proofs,
) -> (
    Bytes,
    Bytes,
    (Bytes, contract::HeightData),
    contract::HeightData,
) {
    let into_bytes = |v: Option<&CommitmentProofBytes>| match v {
        Some(v) => {
            let v: Vec<_> = v.clone().into();
            v.into()
        }
        None => Bytes::new(),
    };
    let object_proof = into_bytes(Some(value.object_proof()));
    let client_proof = into_bytes(value.client_proof().into());
    let (consensus_proof, consensus_height) = match value.consensus_proof() {
        Some(v) => {
            let height = v.height().into();
            let proof = into_bytes(Some(v.proof()));
            (proof, height)
        }
        None => (Bytes::new(), contract::HeightData::default()),
    };
    let height = value.height().into();
    (
        object_proof,
        client_proof,
        (consensus_proof, consensus_height),
        height,
    )
}

impl From<Height> for contract::HeightData {
    fn from(value: Height) -> Self {
        Self {
            revision_number: value.revision_number(),
            revision_height: value.revision_height(),
        }
    }
}

impl From<&CommitmentPrefix> for contract::MerklePrefixData {
    fn from(value: &CommitmentPrefix) -> Self {
        Self {
            key_prefix: value.clone().into_vec().into(),
        }
    }
}

impl From<contract::HeightData> for Height {
    fn from(value: contract::HeightData) -> Self {
        Self::new(value.revision_number, value.revision_height).unwrap()
    }
}

impl From<Counterparty> for contract::CounterpartyData {
    fn from(value: Counterparty) -> Self {
        let client_id: String = value.client_id().as_str().into();
        Self {
            client_id: value.client_id().as_str().into(),
            connection_id: match value.connection_id() {
                Some(id) => id.as_str().into(),
                None => String::from(""),
            },
            prefix: value.prefix().into(),
        }
    }
}

impl From<ics03_connection::version::Version> for contract::VersionData {
    fn from(value: ics03_connection::version::Version) -> Self {
        Self {
            identifier: value.identifier,
            features: value.features,
        }
    }
}

impl From<MsgConnectionOpenInit> for contract::MsgConnectionOpenInit {
    fn from(value: MsgConnectionOpenInit) -> Self {
        Self {
            client_id: value.client_id.as_str().into(),
            counterparty: value.counterparty.into(),
            delay_period: value.delay_period.as_nanos() as u64,
        }
    }
}

impl TryFrom<Any> for contract::MsgConnectionOpenInit {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgConnectionOpenInit::from_any(value)
            .map_err(|e| Error::protobuf_decode(conn_open_init::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgConnectionOpenTry> for contract::MsgConnectionOpenTry {
    fn from(value: MsgConnectionOpenTry) -> Self {
        let (object_proof, client_proof, (consensus_proof, consensus_height), height) =
            into_ethers_proofs(value.proofs);
        Self {
            client_id: into_ethers_client_id(Some(value.client_id)),
            previous_connection_id: into_ethers_connection_id(value.previous_connection_id),
            counterparty: value.counterparty.into(),
            delay_period: value.delay_period.as_nanos() as u64,
            counterparty_versions: value
                .counterparty_versions
                .into_iter()
                .map(|v| v.into())
                .collect(),
            proof_height: height,
            proof_init: object_proof,
            proof_client: client_proof,
            proof_consensus: consensus_proof,
            consensus_height,
            client_state_bytes: into_ethers_client_state(value.client_state),
        }
    }
}

impl TryFrom<Any> for contract::MsgConnectionOpenTry {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgConnectionOpenTry::from_any(value)
            .map_err(|e| Error::protobuf_decode(conn_open_try::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<contract::PacketData> for Packet {
    fn from(value: contract::PacketData) -> Self {
        Self {
            sequence: value.sequence.into(),
            source_port: value.source_port.as_str().parse().unwrap(),
            source_channel: value.source_channel.as_str().parse().unwrap(),
            destination_port: value.destination_port.as_str().parse().unwrap(),
            destination_channel: value.destination_channel.as_str().parse().unwrap(),
            data: value.data.as_ref().to_vec(),
            timeout_height: TimeoutHeight::At(value.timeout_height.into()),
            timeout_timestamp: Timestamp::from_nanoseconds(value.timeout_timestamp).unwrap(),
        }
    }
}

impl From<contract::OwnableIBCHandlerEvents> for IbcEvent {
    fn from(value: contract::OwnableIBCHandlerEvents) -> Self {
        use contract::OwnableIBCHandlerEvents::*;
        use ibc_relayer_types::core::ics04_channel::events as channel_events;
        let event = match value {
            // Connection
            OpenInitConnectionFilter(event) => {
                let attributes = into_connection_attributes(
                    event.connection_id,
                    event.client_id,
                    event.counterparty_connection_id,
                    event.counterparty_client_id,
                );
                IbcEvent::OpenInitConnection(attributes.into())
            }
            OpenTryConnectionFilter(event) => {
                let attributes = into_connection_attributes(
                    event.connection_id,
                    event.client_id,
                    event.counterparty_connection_id,
                    event.counterparty_client_id,
                );
                IbcEvent::OpenTryConnection(attributes.into())
            }
            OpenAckConnectionFilter(event) => {
                let attributes = into_connection_attributes(
                    event.connection_id,
                    event.client_id,
                    event.counterparty_connection_id,
                    event.counterparty_client_id,
                );
                IbcEvent::OpenAckConnection(attributes.into())
            }
            OpenConfirmConnectionFilter(event) => {
                let attributes = into_connection_attributes(
                    event.connection_id,
                    event.client_id,
                    event.counterparty_connection_id,
                    event.counterparty_client_id,
                );
                IbcEvent::OpenConfirmConnection(attributes.into())
            }

            // Channel
            OpenInitChannelFilter(event) => {
                let event = channel_events::OpenInit {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: match event.channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::OpenInitChannel(event)
            }
            OpenTryChannelFilter(event) => {
                let event = channel_events::OpenTry {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: match event.channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::OpenTryChannel(event)
            }
            OpenAckChannelFilter(event) => {
                let event = channel_events::OpenAck {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: match event.channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::OpenAckChannel(event)
            }
            OpenConfirmChannelFilter(event) => {
                let event = channel_events::OpenConfirm {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: match event.channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::OpenConfirmChannel(event)
            }
            CloseInitChannelFilter(event) => {
                let event = channel_events::CloseInit {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: event.channel_id.parse().unwrap(),
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::CloseInitChannel(event)
            }
            CloseConfirmChannelFilter(event) => {
                let event = channel_events::CloseConfirm {
                    port_id: event.port_id.as_str().parse().unwrap(),
                    channel_id: match event.channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                    connection_id: event.connection_id.parse().unwrap(),
                    counterparty_port_id: event.counterparty_port_id.parse().unwrap(),
                    counterparty_channel_id: match event.counterparty_channel_id.as_str() {
                        "" => None,
                        s => Some(s.parse().unwrap()),
                    },
                };
                IbcEvent::CloseConfirmChannel(event)
            }
            SendPacketFilter(event) => {
                let event = channel_events::SendPacket {
                    packet: event.packet.into(),
                };
                IbcEvent::SendPacket(event)
            }
            ReceivePacketFilter(event) => {
                let event = channel_events::ReceivePacket {
                    packet: event.packet.into(),
                };
                IbcEvent::ReceivePacket(event)
            }

            AcknowledgePacketFilter(event) => {
                let event = channel_events::AcknowledgePacket {
                    packet: event.packet.into(),
                };
                IbcEvent::AcknowledgePacket(event)
            }
            WriteAcknowledgementFilter(event) => todo!(),
            CreateClientFilter(_) => todo!(),
            OwnershipTransferredFilter(_) => todo!(),
            UpdateClientFilter(_) => todo!(),
        };
        event
    }
}

fn into_connection_attributes(
    connection_id: String,
    client_id: String,
    counterparty_connection_id: String,
    counterparty_client_id: String,
) -> ics03_connection::events::Attributes {
    ics03_connection::events::Attributes {
        connection_id: match connection_id.as_str() {
            "" => None,
            s => Some(s.parse().unwrap()),
        },
        client_id: client_id.as_str().parse().unwrap(),
        counterparty_connection_id: match counterparty_connection_id.as_str() {
            "" => None,
            s => Some(s.parse().unwrap()),
        },
        counterparty_client_id: counterparty_client_id.as_str().parse().unwrap(),
    }
}
