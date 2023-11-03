use ckb_ics_axon::handler::IbcPacket;
use ckb_ics_axon::handler::PacketStatus;
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::message::MsgAckPacket as CkbMsgAckPacket;
use ckb_ics_axon::message::MsgRecvPacket as CkbMsgRecvPacket;
use ckb_ics_axon::message::MsgType;
use ckb_ics_axon::object::{Ordering, Packet as CkbPacket};
use ckb_ics_axon::{ChannelArgs, PacketArgs};
use ckb_types::packed::BytesOpt;
use ibc_relayer_types::core::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
use ibc_relayer_types::core::ics04_channel::msgs::recv_packet::MsgRecvPacket;
use ibc_relayer_types::core::ics04_channel::packet::Packet;

use super::{CkbTxInfo, MsgToTxConverter, TxBuilder};
use crate::chain::ckb4ibc::utils::{
    convert_port_id_to_array, convert_proof, extract_client_id_by_connection_id,
    get_channel_lock_script, get_channel_number, get_client_outpoint, get_encoded_object,
    get_packet_lock_script,
};
use crate::chain::SEC_TO_NANO;
use crate::error::Error;

fn convert_ibc_packet(packet: &Packet) -> CkbPacket {
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
        data: packet.data.clone(),
        timeout_height: packet.timeout_height.commitment_revision_height(),
        timeout_timestamp: packet.timeout_timestamp.nanoseconds() / SEC_TO_NANO,
    }
}

pub fn convert_recv_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgRecvPacket,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_id = msg.packet.destination_channel.clone();
    let old_channel_end =
        converter.get_ibc_channel(&channel_id, Some(&msg.packet.destination_port))?;
    let mut new_channel_end = old_channel_end.clone();

    let packet = convert_ibc_packet(&msg.packet);
    match old_channel_end.order {
        Ordering::Ordered => new_channel_end.sequence.next_sequence_recvs += 1,
        Ordering::Unordered => {
            if new_channel_end
                .sequence
                .received_sequences
                .contains(&packet.sequence)
            {
                return Err(Error::recv_packet(
                    channel_id,
                    format!("packet({}) has contained", packet.sequence),
                ));
            }
            new_channel_end
                .sequence
                .received_sequences
                .push(packet.sequence);
        }
        Ordering::Unknown => return Err(Error::other("channel ordering must be Order or Unorder")),
    }

    let recv_packet = CkbMsgRecvPacket {
        proofs: convert_proof(msg.proofs)?,
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgRecvPacket,
        content: rlp::encode(&recv_packet).to_vec(),
    };

    let port_id = convert_port_id_to_array(&msg.packet.destination_port)?;
    let channel_number = get_channel_number(&channel_id)?;
    let packet_args = PacketArgs {
        channel_id: channel_number,
        port_id,
        sequence: packet.sequence,
    };

    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&new_channel_end.connection_hops[0], converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: channel_number,
        port_id,
    };

    let old_channel = get_encoded_object(old_channel_end);
    let new_channel = get_encoded_object(&new_channel_end);
    let ibc_packet = get_encoded_object(&IbcPacket {
        packet,
        tx_hash: None,
        status: PacketStatus::Recv,
        ack: None,
    });

    let (channel_input, mut input_capacity) =
        converter.get_ibc_channel_input(&channel_id, &msg.packet.destination_port)?;
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let packet_lock = get_packet_lock_script(converter, packet_args.to_args());

    let mut packet_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(channel_input.clone())
        .witness(old_channel.witness, new_channel.witness);
    let mut write_ack_witness = BytesOpt::default();

    // fetch useless packet cell as input to save capacity
    let useless_write_ack_packet = converter.require_useless_write_ack_packet(15); // TODO: make block number gap setup in config
    if let Some((packet, input, capacity)) = useless_write_ack_packet {
        tracing::info!(
            "use useless WriteAck({}) to save CKB capacity",
            packet.packet.sequence,
        );
        let write_ack_packet = get_encoded_object(&packet);
        write_ack_witness = write_ack_packet.witness;
        packet_tx = packet_tx
            .cell_dep(converter.get_packet_contract_outpoint().clone())
            .input(input);
        input_capacity += capacity;
    }

    let packet_tx = packet_tx
        .output(channel_lock, new_channel.data)
        .output(packet_lock, ibc_packet.data)
        .witness(write_ack_witness, ibc_packet.witness)
        .build();

    Ok(CkbTxInfo {
        unsigned_tx: Some(packet_tx),
        envelope,
        input_capacity,
        event: None,
    })
}

pub fn convert_ack_packet_to_tx<C: MsgToTxConverter>(
    msg: MsgAcknowledgement,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_id = msg.packet.source_channel.clone();
    let old_channel_end = converter.get_ibc_channel(&channel_id, Some(&msg.packet.source_port))?;
    let mut new_channel_end = old_channel_end.clone();

    match old_channel_end.order {
        Ordering::Ordered => new_channel_end.sequence.next_sequence_acks += 1,
        Ordering::Unordered => {}
        Ordering::Unknown => return Err(Error::other("channel ordering must be Order or Unorder")),
    }

    let old_channel = get_encoded_object(old_channel_end);
    let new_channel = get_encoded_object(&new_channel_end);

    let ack_packet = CkbMsgAckPacket {
        proofs: convert_proof(msg.proofs)?,
    };
    let envelope = Envelope {
        msg_type: MsgType::MsgAckPacket,
        content: rlp::encode(&ack_packet).to_vec(),
    };

    let channel_number = get_channel_number(&channel_id)?;
    let packet = convert_ibc_packet(&msg.packet);
    let port_id = convert_port_id_to_array(&msg.packet.source_port)?;
    let packet_args = PacketArgs {
        sequence: packet.sequence,
        channel_id: channel_number,
        port_id,
    };

    let new_packet = get_encoded_object(&IbcPacket {
        packet,
        tx_hash: None,
        status: PacketStatus::Ack,
        ack: Some(msg.acknowledgement.into()),
    });
    let (channel_input, channel_capacity) =
        converter.get_ibc_channel_input(&channel_id, &msg.packet.source_port)?;
    let (old_packet_input, packet_capacity) = converter.get_ibc_packet_input(
        &channel_id,
        &msg.packet.source_port,
        &msg.packet.sequence,
    )?;
    let old_ibc_packet =
        converter.get_ibc_packet(&channel_id, &msg.packet.source_port, &msg.packet.sequence)?;
    let old_packet = get_encoded_object(old_ibc_packet);

    let (client_cell_type_args, client_id) =
        extract_client_id_by_connection_id(&new_channel_end.connection_hops[0], converter)?;
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: channel_number,
        port_id,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let packet_lock = get_packet_lock_script(converter, packet_args.to_args());

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .cell_dep(converter.get_packet_contract_outpoint().clone())
        .input(channel_input.clone())
        .input(old_packet_input.clone())
        .output(channel_lock, new_channel.data)
        .output(packet_lock, new_packet.data)
        .witness(old_channel.witness, new_channel.witness)
        .witness(old_packet.witness, new_packet.witness)
        .build();

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: channel_capacity + packet_capacity,
        event: None,
    })
}
