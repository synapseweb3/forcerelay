use ethers::types::Bytes;
use ibc_proto::google::protobuf::Any;
use ibc_relayer_types::{
    clients::{
        ics07_axon::client_state::AXON_CLIENT_STATE_TYPE_URL,
        ics07_ckb::client_state::CKB_CLIENT_STATE_TYPE_URL,
    },
    core::{
        ics02_client::{
            client_type::ClientType,
            events as client_events,
            msgs::create_client::{self, MsgCreateClient},
        },
        ics03_connection::{
            self,
            connection::{self, ConnectionEnd, IdentifiedConnectionEnd},
            msgs::{
                conn_open_ack::{self, MsgConnectionOpenAck},
                conn_open_confirm::{self, MsgConnectionOpenConfirm},
                conn_open_init::{self, MsgConnectionOpenInit},
                conn_open_try::{self, MsgConnectionOpenTry},
            },
        },
        ics04_channel::{
            self,
            channel::ChannelEnd,
            channel::{self, IdentifiedChannelEnd},
            msgs::{
                acknowledgement::{self, MsgAcknowledgement},
                chan_close_confirm::{self, MsgChannelCloseConfirm},
                chan_close_init::{self, MsgChannelCloseInit},
                chan_open_ack::{self, MsgChannelOpenAck},
                chan_open_confirm::{self, MsgChannelOpenConfirm},
                chan_open_init::{self, MsgChannelOpenInit},
                chan_open_try::{self, MsgChannelOpenTry},
                recv_packet::{self, MsgRecvPacket},
            },
            packet::Packet,
            timeout::TimeoutHeight,
        },
        ics23_commitment::commitment::{CommitmentPrefix, CommitmentProofBytes},
        ics24_host::identifier::{ChannelId, ClientId, ConnectionId},
    },
    events::IbcEvent,
    proofs::Proofs,
    timestamp::Timestamp,
    tx_msg::Msg,
    Height,
};

use super::contract;
use crate::error::Error;

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

fn into_ethers_channel_id(value: Option<ChannelId>) -> String {
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

impl From<connection::Counterparty> for contract::CounterpartyData {
    fn from(value: connection::Counterparty) -> Self {
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

impl From<contract::CounterpartyData> for connection::Counterparty {
    fn from(value: contract::CounterpartyData) -> Self {
        Self::new(
            value.client_id.as_str().parse().unwrap(),
            if value.connection_id.is_empty() {
                None
            } else {
                Some(value.connection_id.as_str().parse().unwrap())
            },
            value
                .prefix
                .key_prefix
                .as_ref()
                .to_vec()
                .try_into()
                .unwrap(),
        )
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

impl From<channel::Counterparty> for contract::ChannelCounterpartyData {
    fn from(value: channel::Counterparty) -> Self {
        Self {
            port_id: value.port_id.as_str().into(),
            channel_id: match value.channel_id {
                Some(id) => id.as_str().into(),
                None => String::from(""),
            },
        }
    }
}

impl From<contract::ChannelCounterpartyData> for channel::Counterparty {
    fn from(value: contract::ChannelCounterpartyData) -> Self {
        Self {
            port_id: value.port_id.as_str().parse().unwrap(),
            channel_id: if value.channel_id.is_empty() {
                None
            } else {
                Some(value.channel_id.as_str().parse().unwrap())
            },
        }
    }
}

impl From<ChannelEnd> for contract::ChannelData {
    fn from(value: ChannelEnd) -> Self {
        Self {
            state: value.state as u8,
            ordering: value.ordering as u8,
            counterparty: value.remote.into(),
            connection_hops: value
                .connection_hops
                .into_iter()
                .map(|h| h.as_str().into())
                .collect(),
            version: value.version.0,
        }
    }
}

impl From<contract::ChannelData> for ChannelEnd {
    fn from(value: contract::ChannelData) -> Self {
        Self {
            state: channel::State::from_i32(value.state as i32).unwrap(),
            ordering: channel::Order::from_i32(value.ordering as i32).unwrap(),
            remote: value.counterparty.into(),
            connection_hops: value
                .connection_hops
                .iter()
                .map(|s| s.parse())
                .collect::<Result<Vec<ConnectionId>, _>>()
                .unwrap(),
            version: ics04_channel::version::Version::new(value.version),
        }
    }
}

impl From<contract::ConnectionEndData> for ConnectionEnd {
    fn from(value: contract::ConnectionEndData) -> Self {
        Self::new(
            connection::State::from_i32(value.state as i32).unwrap(),
            value.client_id.parse().unwrap(),
            value.counterparty.into(),
            value
                .versions
                .into_iter()
                .map(|v| ics03_connection::version::Version {
                    identifier: v.identifier,
                    features: v.features,
                })
                .collect::<Vec<_>>(),
            std::time::Duration::new(value.delay_period, 0),
        )
    }
}

impl From<contract::IdentifiedChannelData> for IdentifiedChannelEnd {
    fn from(value: contract::IdentifiedChannelData) -> Self {
        let channel_end = ChannelEnd {
            state: channel::State::from_i32(value.state as i32).unwrap(),
            ordering: channel::Order::from_i32(value.ordering as i32).unwrap(),
            remote: value.counterparty.into(),
            connection_hops: value
                .connection_hops
                .iter()
                .map(|s| s.parse())
                .collect::<Result<Vec<ConnectionId>, _>>()
                .unwrap(),
            version: ics04_channel::version::Version::new(value.version),
        };
        Self {
            port_id: value.port_id.parse().unwrap(),
            channel_id: value.channel_id.parse().unwrap(),
            channel_end,
        }
    }
}

impl From<contract::IdentifiedConnectionEndData> for IdentifiedConnectionEnd {
    fn from(value: contract::IdentifiedConnectionEndData) -> Self {
        Self {
            connection_id: value.connection_id.parse().unwrap(),
            connection_end: value.connection_end.into(),
        }
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

impl From<Packet> for contract::PacketData {
    fn from(value: Packet) -> Self {
        Self {
            sequence: value.sequence.into(),
            source_port: value.source_port.as_str().into(),
            source_channel: value.source_channel.as_str().into(),
            destination_port: value.destination_port.as_str().into(),
            destination_channel: value.destination_channel.as_str().into(),
            data: value.data.into(),
            timeout_height: match value.timeout_height {
                TimeoutHeight::At(h) => h.into(),
                TimeoutHeight::Never => Default::default(),
            },
            timeout_timestamp: value.timeout_timestamp.nanoseconds(),
        }
    }
}

impl TryFrom<MsgCreateClient> for contract::MsgCreateClient {
    type Error = Error;

    fn try_from(value: MsgCreateClient) -> Result<Self, Self::Error> {
        let client_type = match value.client_state.type_url.as_str() {
            AXON_CLIENT_STATE_TYPE_URL => ClientType::Axon.as_str(),
            CKB_CLIENT_STATE_TYPE_URL => ClientType::Ckb4Ibc.as_str(),
            type_url => {
                return Err(Error::other_error(format!(
                    "unsupported client state type_url: {type_url}"
                )))
            }
        };
        Ok(Self {
            client_type: client_type.to_owned(),
            client_state: value.client_state.value.into(),
            consensus_state: value.consensus_state.value.into(),
        })
    }
}

impl TryFrom<Any> for contract::MsgCreateClient {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        MsgCreateClient::from_any(value)
            .map_err(|e| Error::protobuf_decode(create_client::TYPE_URL.into(), e))?
            .try_into()
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
            client_state: into_ethers_client_state(value.client_state),
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

impl From<MsgConnectionOpenAck> for contract::MsgConnectionOpenAck {
    fn from(value: MsgConnectionOpenAck) -> Self {
        let (object_proof, client_proof, (consensus_proof, consensus_height), height) =
            into_ethers_proofs(value.proofs);
        Self {
            connection_id: value.connection_id.as_str().into(),
            counterparty_connection_id: value.counterparty_connection_id.as_str().into(),
            version: value.version.into(),
            client_state: into_ethers_client_state(value.client_state),
            proof_height: height,
            proof_try: object_proof,
            proof_client: client_proof,
            proof_consensus: consensus_proof,
            consensus_height,
        }
    }
}

impl TryFrom<Any> for contract::MsgConnectionOpenAck {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgConnectionOpenAck::from_any(value)
            .map_err(|e| Error::protobuf_decode(conn_open_ack::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgConnectionOpenConfirm> for contract::MsgConnectionOpenConfirm {
    fn from(value: MsgConnectionOpenConfirm) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            connection_id: value.connection_id.as_str().into(),
            proof_height: height,
            proof_ack: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgConnectionOpenConfirm {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgConnectionOpenConfirm::from_any(value)
            .map_err(|e| Error::protobuf_decode(conn_open_confirm::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelOpenInit> for contract::MsgChannelOpenInit {
    fn from(value: MsgChannelOpenInit) -> Self {
        Self {
            port_id: value.port_id.as_str().into(),
            channel: value.channel.into(),
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelOpenInit {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelOpenInit::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_open_init::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelOpenTry> for contract::MsgChannelOpenTry {
    fn from(value: MsgChannelOpenTry) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            port_id: value.port_id.as_str().into(),
            previous_channel_id: into_ethers_channel_id(value.previous_channel_id),
            channel: value.channel.into(),
            counterparty_version: value.counterparty_version.to_string(),
            proof_height: height,
            proof_init: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelOpenTry {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelOpenTry::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_open_try::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelOpenAck> for contract::MsgChannelOpenAck {
    fn from(value: MsgChannelOpenAck) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            port_id: value.port_id.as_str().into(),
            channel_id: value.channel_id.as_str().into(),
            counterparty_channel_id: value.counterparty_channel_id.as_str().into(),
            counterparty_version: value.counterparty_version.to_string(),
            proof_height: height,
            proof_try: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelOpenAck {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelOpenAck::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_open_ack::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelOpenConfirm> for contract::MsgChannelOpenConfirm {
    fn from(value: MsgChannelOpenConfirm) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            port_id: value.port_id.as_str().into(),
            channel_id: value.channel_id.as_str().into(),
            proof_height: height,
            proof_ack: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelOpenConfirm {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelOpenConfirm::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_open_confirm::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelCloseInit> for contract::MsgChannelCloseInit {
    fn from(value: MsgChannelCloseInit) -> Self {
        Self {
            port_id: value.port_id.as_str().into(),
            channel_id: value.channel_id.as_str().into(),
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelCloseInit {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelCloseInit::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_close_init::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgChannelCloseConfirm> for contract::MsgChannelCloseConfirm {
    fn from(value: MsgChannelCloseConfirm) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            port_id: value.port_id.as_str().into(),
            channel_id: value.channel_id.as_str().into(),
            proof_init: object_proof,
            proof_height: height,
        }
    }
}

impl TryFrom<Any> for contract::MsgChannelCloseConfirm {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgChannelCloseConfirm::from_any(value)
            .map_err(|e| Error::protobuf_decode(chan_close_confirm::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgRecvPacket> for contract::MsgPacketRecv {
    fn from(value: MsgRecvPacket) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            packet: value.packet.into(),
            proof_height: height,
            proof: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgPacketRecv {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgRecvPacket::from_any(value)
            .map_err(|e| Error::protobuf_decode(recv_packet::TYPE_URL.into(), e))?
            .into())
    }
}

impl From<MsgAcknowledgement> for contract::MsgPacketAcknowledgement {
    fn from(value: MsgAcknowledgement) -> Self {
        let (object_proof, _, _, height) = into_ethers_proofs(value.proofs);
        Self {
            packet: value.packet.into(),
            acknowledgement: value.acknowledgement.as_ref().to_vec().into(),
            proof_height: height,
            proof: object_proof,
        }
    }
}

impl TryFrom<Any> for contract::MsgPacketAcknowledgement {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgAcknowledgement::from_any(value)
            .map_err(|e| Error::protobuf_decode(acknowledgement::TYPE_URL.into(), e))?
            .into())
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
            // Packet
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
            WriteAcknowledgementFilter(_) => todo!(),
            CreateClientFilter(event) => {
                let client_id: ClientId = event.client_id.parse().unwrap();
                let client_type = client_id.clone().into();
                assert!(client_type != ClientType::Mock);
                let event = client_events::CreateClient(client_events::Attributes {
                    client_id,
                    client_type,
                    consensus_height: Height::default(),
                });
                IbcEvent::CreateClient(event)
            }
            UpdateClientFilter(event) => {
                let client_id: ClientId = event.client_id.parse().unwrap();
                let client_type = client_id.clone().into();
                assert!(client_type != ClientType::Mock);
                let event = client_events::UpdateClient {
                    common: client_events::Attributes {
                        client_id,
                        client_type,
                        consensus_height: Height::default(),
                    },
                    header: None,
                };
                IbcEvent::UpdateClient(event)
            }
            OwnershipTransferredFilter(_) => todo!(),
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
