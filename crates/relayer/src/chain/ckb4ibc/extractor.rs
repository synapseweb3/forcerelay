use std::str::FromStr;
use std::time::Duration;

use crate::chain::SEC_TO_NANO;
use crate::error::Error;

use ckb_ics_axon::handler::{IbcChannel as CkbIbcChannel, IbcConnections, IbcPacket};
use ckb_ics_axon::message::{Envelope, MsgType};
use ckb_ics_axon::object::{
    ConnectionEnd as CkbConnectionEnd, Ordering as CkbOrdering, Packet as CkbPacket,
    State as CkbState,
};
use ckb_jsonrpc_types::TransactionView;
use ckb_types::packed::WitnessArgs;
use ckb_types::prelude::Entity;
use ibc_relayer_types::core::ics02_client::height::Height;
use ibc_relayer_types::core::ics03_connection::connection::{
    ConnectionEnd, IdentifiedConnectionEnd,
};
use ibc_relayer_types::core::ics03_connection::connection::{
    Counterparty as ConnectionCounterparty, State as ConnectionState,
};
use ibc_relayer_types::core::ics03_connection::version::Version as ConnVersion;
use ibc_relayer_types::core::ics04_channel::channel::{
    ChannelEnd, Counterparty as ChannelCounterparty, IdentifiedChannelEnd, Order,
    State as ChannelState,
};
use ibc_relayer_types::core::ics04_channel::packet::Packet;
use ibc_relayer_types::core::ics04_channel::timeout::TimeoutHeight;
use ibc_relayer_types::core::ics04_channel::version::Version as ChanVersion;
use ibc_relayer_types::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc_relayer_types::timestamp::Timestamp;

use super::utils::{generate_channel_id, generate_connection_id};

pub fn extract_channel_end_from_tx(
    tx: &TransactionView,
) -> Result<(IdentifiedChannelEnd, CkbIbcChannel), Error> {
    let idx = get_object_index(tx, ObjectType::ChannelEnd)?;
    let witness = tx.inner.witnesses.get(idx).unwrap();
    let witness_args = WitnessArgs::from_slice(witness.as_bytes())
        .map_err(|_| Error::ckb_decode_witness_args())?;
    let ckb_channel_end =
        rlp::decode::<CkbIbcChannel>(&witness_args.output_type().to_opt().unwrap().raw_data())
            .map_err(|_| Error::extract_chan_tx_error(tx.hash.to_string()))?;

    let channel_end = convert_channel_end(ckb_channel_end.clone())?;
    Ok((channel_end, ckb_channel_end))
}

pub fn extract_ibc_connections_from_tx(tx: &TransactionView) -> Result<IbcConnections, Error> {
    let idx = get_object_index(tx, ObjectType::IbcConnections)?;
    let witness = tx.inner.witnesses.get(idx).unwrap();
    let witness_args = WitnessArgs::from_slice(witness.as_bytes()).unwrap();
    let ibc_connection_cells =
        rlp::decode::<IbcConnections>(&witness_args.output_type().to_opt().unwrap().raw_data())
            .map_err(|_| Error::extract_conn_tx_error(tx.hash.to_string()))?;

    Ok(ibc_connection_cells)
}

pub fn extract_connections_from_tx(
    tx: &TransactionView,
    prefix: &CommitmentPrefix,
) -> Result<(Vec<IdentifiedConnectionEnd>, IbcConnections), Error> {
    let ibc_connection_cell = extract_ibc_connections_from_tx(tx)?;
    let result = ibc_connection_cell
        .connections
        .iter()
        .enumerate()
        .flat_map(|(idx, connection)| convert_connection_end(connection.clone(), idx, prefix))
        .collect();
    Ok((result, ibc_connection_cell))
}

pub fn extract_ibc_packet_from_tx(tx: &TransactionView) -> Result<(IbcPacket, Vec<u8>), Error> {
    let envelope = get_envelope(tx)?;
    let idx = navigate(&envelope.msg_type, &ObjectType::IbcPacket);
    let witness = tx.inner.witnesses.get(idx).unwrap();
    let witness_args = WitnessArgs::from_slice(witness.as_bytes())
        .map_err(|_| Error::ckb_decode_witness_args())?;
    let ibc_packet =
        rlp::decode::<IbcPacket>(&witness_args.output_type().to_opt().unwrap().raw_data())
            .map_err(|_| Error::extract_chan_tx_error(tx.hash.to_string()))?;
    Ok((ibc_packet, envelope.content))
}

pub fn extract_packet_from_tx(tx: &TransactionView) -> Result<(Packet, Option<Vec<u8>>), Error> {
    let (packet, _) = extract_ibc_packet_from_tx(tx)?;
    Ok((convert_packet(packet.packet)?, packet.ack))
}

fn navigate(t: &MsgType, object_type: &ObjectType) -> usize {
    match (t, object_type) {
        (MsgType::MsgClientCreate, ObjectType::IbcConnections) => 0,
        (MsgType::MsgConnectionOpenInit, ObjectType::IbcConnections) => 0,
        (MsgType::MsgConnectionOpenTry, ObjectType::IbcConnections) => 0,
        (MsgType::MsgConnectionOpenAck, ObjectType::IbcConnections) => 0,
        (MsgType::MsgConnectionOpenConfirm, ObjectType::IbcConnections) => 0,
        (MsgType::MsgChannelOpenInit, ObjectType::IbcConnections) => 0,
        (MsgType::MsgChannelOpenTry, ObjectType::IbcConnections) => 0,
        (MsgType::MsgChannelOpenInit, ObjectType::ChannelEnd) => 1,
        (MsgType::MsgChannelOpenTry, ObjectType::ChannelEnd) => 1,
        (MsgType::MsgChannelOpenAck, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgChannelOpenConfirm, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgChannelCloseInit, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgChannelCloseConfirm, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgSendPacket, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgRecvPacket, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgAckPacket, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgWriteAckPacket, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgTimeoutPacket, ObjectType::ChannelEnd) => 0,
        (MsgType::MsgSendPacket, ObjectType::IbcPacket) => 1,
        (MsgType::MsgRecvPacket, ObjectType::IbcPacket) => 1,
        (MsgType::MsgAckPacket, ObjectType::IbcPacket) => 1,
        (MsgType::MsgWriteAckPacket, ObjectType::IbcPacket) => 1,
        (MsgType::MsgTimeoutPacket, ObjectType::IbcPacket) => 1,
        _ => unreachable!("navigate: {t:?}, {object_type:?}"),
    }
}

fn convert_connection_end(
    connection: CkbConnectionEnd,
    idx: usize,
    prefix: &CommitmentPrefix,
) -> Result<IdentifiedConnectionEnd, Error> {
    let connection_id = generate_connection_id(idx as u16, &connection.client_id);
    let state = match connection.state {
        CkbState::Unknown => ConnectionState::Uninitialized,
        CkbState::Init => ConnectionState::Init,
        CkbState::OpenTry => ConnectionState::TryOpen,
        CkbState::Open => ConnectionState::Open,
        _ => ConnectionState::Uninitialized,
    };
    let client_id = {
        let s = connection.client_id;
        ClientId::from_str(&s).map_err(|_| Error::ckb_client_id_invalid(s))
    }?;
    let remote_client_id = {
        let id = connection.counterparty.client_id;
        ClientId::from_str(&id).map_err(|_| Error::ckb_client_id_invalid(id))
    }?;
    let remote_connection_id = connection
        .counterparty
        .connection_id
        .map(|c| ConnectionId::from_str(&c).unwrap());
    let delay_period = connection.delay_period;
    let result = IdentifiedConnectionEnd {
        connection_id,
        connection_end: ConnectionEnd::new(
            state,
            client_id,
            ConnectionCounterparty::new(remote_client_id, remote_connection_id, prefix.clone()),
            vec![ConnVersion::default()],
            Duration::from_secs(delay_period),
        ),
    };
    Ok(result)
}

fn convert_channel_end(ckb_channel_end: CkbIbcChannel) -> Result<IdentifiedChannelEnd, Error> {
    let state = match ckb_channel_end.state {
        CkbState::Unknown => ChannelState::Uninitialized,
        CkbState::Init => ChannelState::Init,
        CkbState::OpenTry => ChannelState::TryOpen,
        CkbState::Open => ChannelState::Open,
        CkbState::Closed => ChannelState::Closed,
        CkbState::Frozen => panic!(),
    };
    let ordering = match ckb_channel_end.order {
        CkbOrdering::Unknown => Order::None,
        CkbOrdering::Unordered => Order::Unordered,
        CkbOrdering::Ordered => Order::Ordered,
    };
    let remote_port_id = PortId::from_str(&ckb_channel_end.counterparty.port_id)
        .map_err(|_| Error::convert_channel_end())?;
    let remote_channel_id = if ckb_channel_end.counterparty.channel_id.is_empty() {
        None
    } else {
        Some(
            ChannelId::from_str(&ckb_channel_end.counterparty.channel_id)
                .map_err(|_| Error::convert_channel_end())?,
        )
    };
    let remote = ChannelCounterparty {
        port_id: remote_port_id,
        channel_id: remote_channel_id,
    };
    let connection_hops = {
        ckb_channel_end
            .connection_hops
            .into_iter()
            .map(|c| c.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::other_error("bad connection_hops".to_owned()))?
    };
    let channel_end = ChannelEnd {
        state,
        ordering,
        remote,
        connection_hops,
        version: ChanVersion::empty(),
    };

    let port_id =
        PortId::from_str(&ckb_channel_end.port_id).map_err(|_| Error::convert_channel_end())?;
    let channel_id = generate_channel_id(ckb_channel_end.number);

    let result = IdentifiedChannelEnd {
        port_id,
        channel_id,
        channel_end,
    };
    Ok(result)
}

fn convert_packet(packet: CkbPacket) -> Result<Packet, Error> {
    fn other_error<E: ToString>(error: E) -> Error {
        Error::other_error(error.to_string())
    }
    Ok(Packet {
        sequence: (packet.sequence as u64).try_into().map_err(other_error)?,
        source_port: PortId::from_str(&packet.source_port_id).map_err(other_error)?,
        source_channel: ChannelId::from_str(&packet.source_channel_id).map_err(other_error)?,
        destination_port: PortId::from_str(&packet.destination_port_id).map_err(other_error)?,
        destination_channel: ChannelId::from_str(&packet.destination_channel_id)
            .map_err(other_error)?,
        timeout_height: if packet.timeout_height == 0 {
            TimeoutHeight::Never
        } else {
            TimeoutHeight::At(Height::from_noncosmos_height(packet.timeout_height))
        },
        timeout_timestamp: Timestamp::from_nanoseconds(packet.timeout_timestamp * SEC_TO_NANO)
            .map_err(other_error)?,
        data: packet.data,
    })
}

#[derive(Debug, Clone, Copy)]
enum ObjectType {
    ChannelEnd,
    IbcConnections,
    IbcPacket,
}

fn get_object_index(tx: &TransactionView, object_type: ObjectType) -> Result<usize, Error> {
    let envelope = get_envelope(tx)?;
    Ok(navigate(&envelope.msg_type, &object_type))
}

pub fn get_envelope(tx: &TransactionView) -> Result<Envelope, Error> {
    let msg = tx.inner.witnesses.last().ok_or(Error::ckb_none_witness())?;

    let bytes = msg.as_bytes();
    let envelope_bytes = WitnessArgs::from_slice(bytes)
        .map_err(|_| Error::ckb_decode_witness_args())?
        .output_type()
        .to_opt()
        .unwrap();
    let envelope_slice = envelope_bytes.raw_data();

    let envelope =
        rlp::decode::<Envelope>(&envelope_slice).map_err(|_| Error::ckb_decode_envelope())?;
    Ok(envelope)
}
