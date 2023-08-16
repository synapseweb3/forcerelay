use ckb_ics_axon::consts::{CHANNEL_CELL_CAPACITY, PACKET_CELL_CAPACITY};
use ckb_ics_axon::handler::IbcPacket;
use ckb_ics_axon::handler::PacketStatus;
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::message::MsgAckPacket as CkbMsgAckPacket;
use ckb_ics_axon::message::MsgRecvPacket as CkbMsgRecvPacket;
use ckb_ics_axon::message::MsgType;
use ckb_ics_axon::object::Packet as CkbPacket;
use ckb_ics_axon::{ChannelArgs, PacketArgs};
use ckb_types::packed::BytesOpt;
use ibc_relayer_types::core::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
use ibc_relayer_types::core::ics04_channel::msgs::recv_packet::MsgRecvPacket;
use ibc_relayer_types::core::ics04_channel::packet::Packet;

use super::{CkbTxInfo, MsgToTxConverter, TxBuilder};
use crate::chain::ckb4ibc::utils::{
    convert_proof, extract_client_id_by_connection_id, get_channel_capacity, get_channel_idx,
    get_channel_lock_script, get_client_outpoint, get_encoded_object, get_packet_capacity,
    get_packet_lock_script,
};
use crate::error::Error;

fn convert_ibc_packet(packet: Packet) -> CkbPacket {
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

pub fn convert_ack_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgAcknowledgement,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_id = msg.packet.source_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id);
    let mut new_channel_end = old_channel_end.clone();
    new_channel_end.sequence.next_sequence_acks += 1;
    let old_channel = get_encoded_object(old_channel_end);
    let new_channel = get_encoded_object(new_channel_end.clone());

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
    let new_packet = get_encoded_object(new_ibc_packet);
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
    let packet_args = PacketArgs {
        channel_id: channel_idx,
        port_id: port_id_in_args,
        sequence: seq,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let packet_lock = get_packet_lock_script(converter, packet_args.to_args());

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint())
        .input(channel_input)
        .input(old_ibc_packet_input)
        .output(channel_lock, get_channel_capacity(), new_channel.data)
        .output(packet_lock, get_packet_capacity(), new_packet.data)
        .witness(old_channel.witness, new_channel.witness)
        .witness(BytesOpt::default(), new_packet.witness)
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

    let old_channel = get_encoded_object(old_channel_end);
    let new_channel = get_encoded_object(new_channel_end.clone());

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
    let packet = get_encoded_object(ibc_packet);
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
    let packet_args = PacketArgs {
        channel_id: channel_idx,
        port_id: port_id_in_args,
        sequence: seq,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let packet_lock = get_packet_lock_script(converter, packet_args.to_args());

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .input(channel_input)
        .output(channel_lock, get_channel_capacity(), new_channel.data)
        .output(packet_lock, get_packet_capacity(), packet.data)
        .witness(old_channel.witness, new_channel.witness)
        .witness(BytesOpt::default(), packet.witness)
        .build();

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: PACKET_CELL_CAPACITY,
        event: None,
    })
}
