use super::MsgToTxConverter;
use crate::chain::ckb4ibc::utils::{convert_proof, get_channel_idx, get_encoded_object};
use crate::error::Error;
use ckb_ics_axon::consts::{CHANNEL_CELL_CAPACITY, CONNECTION_CELL_CAPACITY, PACKET_CELL_CAPACITY};

use ckb_ics_axon::handler::IbcChannel;
use ckb_ics_axon::handler::IbcPacket;
use ckb_ics_axon::handler::PacketStatus;
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::message::MsgAckPacket as CkbMsgAckPacket;
use ckb_ics_axon::message::MsgChannelOpenAck as CkbMsgChannelOpenAck;
use ckb_ics_axon::message::MsgChannelOpenConfirm as CkbMsgChannelOpenConfirm;
use ckb_ics_axon::message::MsgChannelOpenInit as CkbMsgChannelOpenInit;
use ckb_ics_axon::message::MsgChannelOpenTry as CkbMsgChannelOpenTry;
use ckb_ics_axon::message::MsgRecvPacket as CkbMsgRecvPacket;
use ckb_ics_axon::message::MsgType;
use ckb_ics_axon::object::Packet as CkbPacket;
use ckb_ics_axon::object::{ChannelCounterparty, Ordering as CkbOrdering, State as CkbState};
use ckb_ics_axon::{ChannelArgs, PacketArgs};
use ckb_types::core::{Capacity, TransactionView};
use ckb_types::packed::CellDep;
use ckb_types::packed::{CellOutput, Script, WitnessArgs};
use ckb_types::prelude::{Builder, Entity, Pack};
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, Order, State};
use ibc_relayer_types::core::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
use ibc_relayer_types::core::ics04_channel::msgs::recv_packet::MsgRecvPacket;
use ibc_relayer_types::core::ics04_channel::msgs::{
    chan_close_init::MsgChannelCloseInit, chan_open_ack::MsgChannelOpenAck,
    chan_open_confirm::MsgChannelOpenConfirm, chan_open_init::MsgChannelOpenInit,
    chan_open_try::MsgChannelOpenTry,
};
use ibc_relayer_types::core::ics04_channel::packet::Packet;
use ibc_relayer_types::core::ics24_host::identifier::PortId;

pub fn convert_chan_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenInit,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let old_connection_cell = converter.get_ibc_connections();
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let ibc_channel_end = convert_channel_end(msg.channel, msg.port_id.clone(), next_channel_num)?;
    let ibc_channel_end_encoded = get_encoded_object(ibc_channel_end);

    let old_connection_encoded = get_encoded_object(old_connection_cell);
    let new_connection_encoded = get_encoded_object(new_connection_cell);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenInit,
        content: rlp::encode(&CkbMsgChannelOpenInit {}).to_vec(),
    };
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;
    let packed_tx = TransactionView::new_advanced_builder()
        .input(converter.get_ibc_connections_input())
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(
                            ChannelArgs {
                                client_id: converter.get_client_id(),
                                open: false,
                                channel_id: next_channel_num,
                                port_id: msg.port_id.as_bytes().try_into().unwrap(),
                            }
                            .to_args()
                            .pack(),
                        )
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(msg.port_id.to_string().pack())
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(new_connection_encoded.data)
        .output_data(ibc_channel_end_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .witness(
            WitnessArgs::new_builder()
                .output_type(ibc_channel_end_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}

pub fn convert_chan_open_try_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenTry,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let old_connection_cell = converter.get_ibc_connections();
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let ibc_channel_end = convert_channel_end(msg.channel, msg.port_id.clone(), next_channel_num)?;
    let ibc_channel_end_encoded = get_encoded_object(ibc_channel_end);

    let old_connection_encoded = get_encoded_object(old_connection_cell);
    let new_connection_encoded = get_encoded_object(new_connection_cell);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenTry,
        content: rlp::encode(&CkbMsgChannelOpenTry {
            proof_chan_end_on_a: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;
    let packed_tx = TransactionView::new_advanced_builder()
        .input(converter.get_ibc_connections_input())
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_connection_code_hash())
                        .args(converter.get_client_id().to_vec().pack())
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(
                            ChannelArgs {
                                client_id: converter.get_client_id(),
                                open: false,
                                channel_id: next_channel_num,
                                port_id: msg.port_id.as_bytes().try_into().unwrap(),
                            }
                            .to_args()
                            .pack(),
                        )
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(new_connection_encoded.data)
        .output_data(ibc_channel_end_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .witness(
            WitnessArgs::new_builder()
                .output_type(ibc_channel_end_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}

pub fn convert_chan_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenAck,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let channel_idx = get_channel_idx(&msg.channel_id)?;
    let old_channel = converter.get_ibc_channel(&msg.channel_id);
    let new_channel = old_channel.clone();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenAck,
        content: rlp::encode(&CkbMsgChannelOpenAck {
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let lock_args = ChannelArgs {
        client_id: converter.get_client_id(),
        open: true,
        channel_id: channel_idx,
        port_id: msg.port_id.as_bytes().try_into().unwrap(),
    }
    .to_args()
    .pack();

    let old_channel_encoded = get_encoded_object(old_channel);
    let new_channel_encoded = get_encoded_object(new_channel);

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(converter.get_client_outpoint())
                .build(),
        )
        .input(converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id))
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(lock_args)
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_encoded.witness)
                .output_type(new_channel_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CHANNEL_CELL_CAPACITY))
}

pub fn convert_chan_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenConfirm,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let old_channel = converter.get_ibc_channel(&msg.channel_id);
    let new_channel = old_channel.clone();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenConfirm,
        content: rlp::encode(&CkbMsgChannelOpenConfirm {
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let lock_args = ChannelArgs {
        client_id: converter.get_client_id(),
        open: true,
        channel_id: get_channel_idx(&msg.channel_id)?,
        port_id: old_channel.port_id.as_bytes().try_into().unwrap(),
    }
    .to_args();

    let old_channel_encoded = get_encoded_object(old_channel);
    let new_channel_encoded = get_encoded_object(new_channel);

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(converter.get_client_outpoint())
                .build(),
        )
        .input(converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id))
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(lock_args.pack())
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_encoded.witness)
                .output_type(new_channel_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CHANNEL_CELL_CAPACITY))
}

pub fn convert_chan_close_init_to_tx<C: MsgToTxConverter>(
    _msg: MsgChannelCloseInit,
    _converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    todo!()
}

pub fn convert_ack_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgAcknowledgement,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let channel_id = msg.packet.source_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id);
    let mut new_channel_end = old_channel_end.clone();
    new_channel_end.sequence.next_recv_ack += 1;
    let old_channel_end_encoded = get_encoded_object(old_channel_end);
    let new_channel_end_encoded = get_encoded_object(new_channel_end);

    let ckb_msg = CkbMsgAckPacket {
        proofs: convert_proof(msg.proofs)?,
        acknowledgement: msg.acknowledgement.as_ref().to_vec(),
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgAckPacket,
        content: rlp::encode(&ckb_msg).to_vec(),
    };
    let port_id = msg.packet.source_port.clone();
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let channel_input = converter.get_ibc_channel_input(&channel_id, &msg.packet.source_port);
    let sequence = msg.packet.sequence;
    let packet = convert_ibc_packet(msg.packet);
    let seq = packet.sequence;
    let new_ibc_packet = IbcPacket {
        packet,
        tx_hash: None,
        status: PacketStatus::Ack,
    };
    let new_ibc_packet_encoded = get_encoded_object(new_ibc_packet);
    let old_ibc_packet_input =
        converter.get_packet_cell_input(channel_id.clone(), port_id.clone(), sequence);
    let channel_idx = get_channel_idx(&channel_id)?;
    let port_id_in_args: [u8; 32] = port_id.as_bytes().try_into().unwrap();
    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(converter.get_client_outpoint())
                .build(),
        )
        .input(channel_input)
        .input(old_ibc_packet_input)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(
                            ChannelArgs {
                                client_id: converter.get_client_id(),
                                open: true,
                                channel_id: channel_idx,
                                port_id: port_id_in_args,
                            }
                            .to_args()
                            .pack(),
                        )
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(new_channel_end_encoded.data)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_packet_code_hash())
                        .args(
                            PacketArgs {
                                channel_id: channel_idx,
                                port_id: port_id_in_args,
                                sequence: seq,
                                owner: converter.get_packet_owner(),
                            }
                            .to_args()
                            .pack(),
                        ) // todo
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(new_ibc_packet_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_end_encoded.witness)
                .output_type(new_channel_end_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .witness(
            WitnessArgs::new_builder()
                .output_type(new_ibc_packet_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((
        packed_tx,
        envelope,
        CHANNEL_CELL_CAPACITY + PACKET_CELL_CAPACITY,
    ))
}

pub fn convert_recv_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgRecvPacket,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let channel_id = msg.packet.destination_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id);
    let mut new_channel_end = old_channel_end.clone();
    new_channel_end.sequence.next_recv_packet += 1;

    let old_channel_end_encoded = get_encoded_object(old_channel_end);
    let new_channel_end_encoded = get_encoded_object(new_channel_end);

    let ckb_msg = CkbMsgRecvPacket {
        proofs: convert_proof(msg.proofs)?,
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgRecvPacket,
        content: rlp::encode(&ckb_msg).to_vec(),
    };
    let port_id = msg.packet.destination_port.clone();
    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let channel_input = converter.get_ibc_channel_input(&channel_id, &msg.packet.source_port);
    let packet = convert_ibc_packet(msg.packet);
    let seq = packet.sequence;
    let ibc_packet = IbcPacket {
        packet,
        tx_hash: None,
        status: PacketStatus::Recv,
    };
    let ibc_packet_encoded = get_encoded_object(ibc_packet);
    let channel_idx = get_channel_idx(&channel_id)?;
    let port_id_in_args: [u8; 32] = port_id.as_str().as_bytes().try_into().unwrap();
    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(converter.get_client_outpoint())
                .build(),
        )
        .input(channel_input)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(
                            ChannelArgs {
                                client_id: converter.get_client_id(),
                                open: true,
                                channel_id: channel_idx,
                                port_id: port_id_in_args,
                            }
                            .to_args()
                            .pack(),
                        )
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(new_channel_end_encoded.data)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(
                            PacketArgs {
                                channel_id: channel_idx,
                                port_id: port_id_in_args,
                                sequence: seq,
                                owner: converter.get_packet_owner(),
                            }
                            .to_args()
                            .pack(),
                        ) // todo
                        .build(),
                )
                .build_exact_capacity(data_capacity)
                .map_err(|_| Error::capacity())?,
        )
        .output_data(ibc_packet_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_end_encoded.witness)
                .output_type(new_channel_end_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .witness(
            WitnessArgs::new_builder()
                .output_type(ibc_packet_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, PACKET_CELL_CAPACITY))
}

pub fn convert_channel_end(
    channel_end: ChannelEnd,
    port_id: PortId,
    channel_num: u16,
) -> Result<IbcChannel, Error> {
    let state = match channel_end.state {
        State::Uninitialized => CkbState::Unknown,
        State::Init => CkbState::Init,
        State::TryOpen => CkbState::OpenTry,
        State::Open => CkbState::Open,
        State::Closed => CkbState::Closed,
    };
    let order = match channel_end.ordering {
        Order::None => CkbOrdering::Unknown,
        Order::Unordered => CkbOrdering::Unordered,
        Order::Ordered => CkbOrdering::Ordered,
    };
    let port_id = port_id.to_string();
    let connection_hops = channel_end
        .connection_hops
        .into_iter()
        .flat_map(|c| c.as_str().parse::<usize>().ok())
        .collect::<Vec<_>>();

    if connection_hops.is_empty() {
        return Err(Error::empty_connection_hops());
    }

    let remote_port_id = channel_end.remote.port_id.to_string();
    let remote_channel_id = {
        if let Some(id) = channel_end.remote.channel_id {
            id.to_string()
        } else {
            String::from("")
        }
    };

    let counterparty = ChannelCounterparty {
        port_id: remote_port_id,
        channel_id: remote_channel_id,
    };

    let result = IbcChannel {
        num: channel_num,
        port_id,
        state,
        order,
        sequence: Default::default(),
        counterparty,
        connection_hops,
    };
    Ok(result)
}

pub fn convert_ibc_packet(packet: Packet) -> CkbPacket {
    let seq: u64 = packet.sequence.into();
    let source_port_id = packet.source_port.to_string();
    let source_channel_id = packet.source_channel.to_string();
    let destination_port_id = packet.destination_port.to_string();
    let destination_channel_id = packet.destination_channel.to_string();
    CkbPacket {
        sequence: seq as u16,
        source_port_id,
        source_channel_id,
        destination_port_id,
        destination_channel_id,
        data: packet.data,
    }
}
