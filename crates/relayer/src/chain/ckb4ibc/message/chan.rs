use std::str::FromStr;

use super::{CkbTxInfo, MsgToTxConverter};
use crate::chain::ckb4ibc::utils::{
    convert_port_id_to_array, convert_proof, extract_client_id_by_connection_id,
    generate_channel_id, get_channel_capacity, get_channel_idx, get_client_id_from_channel,
    get_client_outpoint, get_connection_capacity, get_connection_lock_script, get_encoded_object,
    get_packet_capacity,
};
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
use ckb_types::core::{DepType, ScriptHashType, TransactionView};
use ckb_types::packed::CellDep;
use ckb_types::packed::{CellOutput, Script, WitnessArgs};
use ckb_types::prelude::{Builder, Entity, Pack};
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, Order, State};
use ibc_relayer_types::core::ics04_channel::events::{OpenAck, OpenConfirm, OpenInit, OpenTry};
use ibc_relayer_types::core::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
use ibc_relayer_types::core::ics04_channel::msgs::recv_packet::MsgRecvPacket;
use ibc_relayer_types::core::ics04_channel::msgs::{
    chan_close_init::MsgChannelCloseInit, chan_open_ack::MsgChannelOpenAck,
    chan_open_confirm::MsgChannelOpenConfirm, chan_open_init::MsgChannelOpenInit,
    chan_open_try::MsgChannelOpenTry,
};
use ibc_relayer_types::core::ics04_channel::packet::Packet;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, PortId};
use ibc_relayer_types::events::IbcEvent;

pub fn convert_chan_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenInit,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_connection_cell =
        converter.get_ibc_connections_by_connection_id(&msg.channel.connection_hops[0])?;
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let ibc_channel_end =
        convert_channel_end(msg.channel.clone(), msg.port_id.clone(), next_channel_num)?;
    let ibc_channel_end_encoded = get_encoded_object(ibc_channel_end);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenInit,
        content: rlp::encode(&CkbMsgChannelOpenInit {}).to_vec(),
    };

    let (client_cell_type_args, client_id) = get_client_id_from_channel(&msg.channel, converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: false,
        channel_id: next_channel_num,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let old_connection_encoded = get_encoded_object(old_connection_cell);
    let new_connection_encoded = get_encoded_object(new_connection_cell);

    let client_outpoint = get_client_outpoint(converter, &client_id)?;
    let ibc_connection_input = converter.get_ibc_connections_input(&client_id)?;
    let connection_lock = get_connection_lock_script(converter.get_config(), Some(client_id))?;

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(client_outpoint)
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_conn_contract_outpoint())
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_chan_contract_outpoint())
                .build(),
        )
        .input(ibc_connection_input)
        .output(
            CellOutput::new_builder()
                .lock(connection_lock)
                .capacity(get_connection_capacity().pack())
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(channel_args.to_args().pack())
                        .hash_type(ScriptHashType::Type.into())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
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
    let event = IbcEvent::OpenInitChannel(OpenInit {
        port_id: msg.port_id,
        channel_id: Some(generate_channel_id(next_channel_num)),
        connection_id: msg.channel.connection_hops[0].clone(),
        counterparty_port_id: msg.channel.remote.port_id,
        counterparty_channel_id: msg.channel.remote.channel_id,
    });
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_chan_open_try_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenTry,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_connection_cell =
        converter.get_ibc_connections_by_connection_id(&msg.channel.connection_hops[0])?;
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let ibc_channel_end =
        convert_channel_end(msg.channel.clone(), msg.port_id.clone(), next_channel_num)?;
    let ibc_channel_end_encoded = get_encoded_object(ibc_channel_end);

    let (client_cell_type_args, client_id) = get_client_id_from_channel(&msg.channel, converter)?;
    let old_connection_encoded = get_encoded_object(old_connection_cell);
    let new_connection_encoded = get_encoded_object(new_connection_cell);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenTry,
        content: rlp::encode(&CkbMsgChannelOpenTry {
            proof_chan_end_on_a: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: false,
        channel_id: next_channel_num,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };
    let ibc_connection_input = converter.get_ibc_connections_input(&client_id)?;
    let client_outpoint = get_client_outpoint(converter, &client_id)?;
    let connection_lock = get_connection_lock_script(converter.get_config(), Some(client_id))?;

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(client_outpoint)
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_conn_contract_outpoint())
                .build(),
        )
        .input(ibc_connection_input)
        .output(
            CellOutput::new_builder()
                .lock(connection_lock)
                .capacity(get_connection_capacity().pack())
                .build(),
        )
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(channel_args.to_args().pack())
                        .hash_type(ScriptHashType::Type.into())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
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
    let event = IbcEvent::OpenTryChannel(OpenTry {
        port_id: msg.port_id,
        channel_id: Some(generate_channel_id(next_channel_num)),
        connection_id: msg.channel.connection_hops[0].clone(),
        counterparty_port_id: msg.channel.remote.port_id,
        counterparty_channel_id: msg.channel.remote.channel_id,
    });
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_chan_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenAck,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_idx = get_channel_idx(&msg.channel_id)?;
    let old_channel = converter.get_ibc_channel(&msg.channel_id);
    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id).unwrap();
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Open;
    new_channel.counterparty.channel_id = msg.counterparty_channel_id.as_str().to_string();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenAck,
        content: rlp::encode(&CkbMsgChannelOpenAck {
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let connection_id = old_channel.connection_hops[0].clone();
    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&connection_id, converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: channel_idx,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let old_channel_encoded = get_encoded_object(old_channel);
    let new_channel_encoded = get_encoded_object(new_channel);

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(get_client_outpoint(converter, &client_id)?)
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_conn_contract_outpoint())
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_chan_contract_outpoint())
                .build(),
        )
        .input(converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id))
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .args(channel_args.to_args().pack())
                        .hash_type(ScriptHashType::Type.into())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
        )
        .output_data(new_channel_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_encoded.witness)
                .output_type(new_channel_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();

    let event = IbcEvent::OpenAckChannel(OpenAck {
        port_id: msg.port_id,
        channel_id: Some(msg.channel_id),
        connection_id: connection_id.parse().unwrap(),
        counterparty_port_id,
        counterparty_channel_id: Some(msg.counterparty_channel_id),
    });
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CHANNEL_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_chan_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenConfirm,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_channel = converter.get_ibc_channel(&msg.channel_id);
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Open;

    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id)
        .map_err(|_| Error::ckb_port_id_invalid(old_channel.counterparty.port_id.clone()))?;
    let counterparty_channel_id =
        ChannelId::from_str(&old_channel.counterparty.channel_id).unwrap();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenConfirm,
        content: rlp::encode(&CkbMsgChannelOpenConfirm {
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let connection_id = old_channel.connection_hops[0].clone();
    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&connection_id, converter)?;
    let lock_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: get_channel_idx(&msg.channel_id)?,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let old_channel_encoded = get_encoded_object(old_channel);
    let new_channel_encoded = get_encoded_object(new_channel);

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(get_client_outpoint(converter, &client_id)?)
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_chan_contract_outpoint())
                .build(),
        )
        .input(converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id))
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .hash_type(ScriptHashType::Type.into())
                        .args(lock_args.to_args().pack())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
        )
        .output_data(new_channel_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_channel_encoded.witness)
                .output_type(new_channel_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    let event = IbcEvent::OpenConfirmChannel(OpenConfirm {
        port_id: msg.port_id,
        channel_id: Some(msg.channel_id),
        connection_id: connection_id.parse().unwrap(),
        counterparty_port_id,
        counterparty_channel_id: Some(counterparty_channel_id),
    });
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CHANNEL_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_chan_close_init_to_tx<C: MsgToTxConverter>(
    _msg: MsgChannelCloseInit,
    _converter: &C,
) -> Result<CkbTxInfo, Error> {
    todo!()
}

pub fn convert_ack_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgAcknowledgement,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_id = msg.packet.source_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id);
    let mut new_channel_end = old_channel_end.clone();
    new_channel_end.sequence.next_sequence_acks += 1;
    let old_channel_end_encoded = get_encoded_object(old_channel_end);
    let new_channel_end_encoded = get_encoded_object(new_channel_end.clone());

    let ckb_msg = CkbMsgAckPacket {
        proofs: convert_proof(msg.proofs)?,
        acknowledgement: msg.acknowledgement.as_ref().to_vec(),
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgAckPacket,
        content: rlp::encode(&ckb_msg).to_vec(),
    };
    let port_id = msg.packet.source_port.clone();

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

    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&new_channel_end.connection_hops[0], converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: channel_idx,
        port_id: port_id_in_args,
    };

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(get_client_outpoint(converter, &client_id)?)
                .build(),
        )
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_chan_contract_outpoint())
                .build(),
        )
        .input(channel_input)
        .input(old_ibc_packet_input)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .hash_type(ScriptHashType::Type.into())
                        .args(channel_args.to_args().pack())
                        .hash_type(ScriptHashType::Type.into())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
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
                            }
                            .to_args()
                            .pack(),
                        ) // todo
                        .build(),
                )
                .capacity(get_packet_capacity().pack())
                .build(),
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
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CHANNEL_CELL_CAPACITY + PACKET_CELL_CAPACITY,
        event: None,
    })
}

pub fn convert_recv_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgRecvPacket,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_id = msg.packet.destination_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id);
    let mut new_channel_end = old_channel_end.clone();
    new_channel_end.sequence.next_sequence_recvs += 1;

    let old_channel_end_encoded = get_encoded_object(old_channel_end);
    let new_channel_end_encoded = get_encoded_object(new_channel_end.clone());

    let ckb_msg = CkbMsgRecvPacket {
        proofs: convert_proof(msg.proofs)?,
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgRecvPacket,
        content: rlp::encode(&ckb_msg).to_vec(),
    };
    let port_id = msg.packet.destination_port.clone();

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

    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&new_channel_end.connection_hops[0], converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: channel_idx,
        port_id: port_id_in_args,
    };

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .out_point(get_client_outpoint(converter, &client_id)?)
                .build(),
        )
        .input(channel_input)
        .output(
            CellOutput::new_builder()
                .lock(
                    Script::new_builder()
                        .code_hash(converter.get_channel_code_hash())
                        .hash_type(ScriptHashType::Type.into())
                        .args(channel_args.to_args().pack())
                        .build(),
                )
                .capacity(get_channel_capacity().pack())
                .build(),
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
                            }
                            .to_args()
                            .pack(),
                        ) // todo
                        .build(),
                )
                .capacity(get_packet_capacity().pack())
                .build(),
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
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: PACKET_CELL_CAPACITY,
        event: None,
    })
}

pub fn convert_channel_end(
    channel_end: ChannelEnd,
    port_id: PortId,
    channel_number: u16,
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
        .map(|v| v.to_string())
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
        number: channel_number,
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
        timeout_height: packet.timeout_height.commitment_revision_height(),
        timeout_timestamp: packet.timeout_timestamp.nanoseconds() / 100000, // use second as unit
    }
}
