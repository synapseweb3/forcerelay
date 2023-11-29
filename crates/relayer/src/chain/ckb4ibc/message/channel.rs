use ckb_ics_axon::commitment::channel_path;
use ckb_ics_axon::handler::{IbcChannel, Sequence};
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::message::MsgChannelCloseConfirm as CkbMsgChannelCloseConfirm;
use ckb_ics_axon::message::MsgChannelCloseInit as CkbMsgChannelCloseInit;
use ckb_ics_axon::message::MsgChannelOpenAck as CkbMsgChannelOpenAck;
use ckb_ics_axon::message::MsgChannelOpenConfirm as CkbMsgChannelOpenConfirm;
use ckb_ics_axon::message::MsgChannelOpenInit as CkbMsgChannelOpenInit;
use ckb_ics_axon::message::MsgChannelOpenTry as CkbMsgChannelOpenTry;
use ckb_ics_axon::message::MsgType;
use ckb_ics_axon::object::{ChannelCounterparty, Ordering as CkbOrdering, State as CkbState};
use ckb_ics_axon::{get_channel_id_str, ChannelArgs};
use ckb_types::packed::BytesOpt;
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, Order, State};
use ibc_relayer_types::core::ics04_channel::events::{
    CloseConfirm, CloseInit, OpenAck, OpenConfirm, OpenInit, OpenTry,
};
use ibc_relayer_types::core::ics04_channel::msgs::{
    chan_close_confirm::MsgChannelCloseConfirm, chan_close_init::MsgChannelCloseInit,
    chan_open_ack::MsgChannelOpenAck, chan_open_confirm::MsgChannelOpenConfirm,
    chan_open_init::MsgChannelOpenInit, chan_open_try::MsgChannelOpenTry,
};
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use ibc_relayer_types::events::IbcEvent;
use std::str::FromStr;

use super::{convert_proof_height, CkbTxInfo, MsgToTxConverter, TxBuilder};
use crate::chain::ckb4ibc::utils::{
    convert_port_id_to_array, get_channel_lock_script, get_channel_number, get_client_outpoint,
    get_connection_lock_script, get_encoded_object,
};
use crate::error::Error;

fn convert_channel_end(
    channel_end: ChannelEnd,
    port_id: PortId,
    remote_connection_id: String,
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
        connection_id: remote_connection_id,
    };

    let sequence = Sequence {
        next_sequence_sends: 1,
        next_sequence_recvs: 1,
        next_sequence_acks: 1,
        ..Default::default()
    };

    let result = IbcChannel {
        number: channel_number,
        port_id,
        state,
        order,
        sequence,
        counterparty,
        connection_hops,
        version: channel_end.version.to_string(),
    };
    Ok(result)
}

pub fn convert_chan_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenInit,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let connection_id = &msg.channel.connection_hops[0];

    let (connection_args, old_connection_cell) =
        converter.get_ibc_connections_by_connection_id(connection_id)?;
    let client_id = connection_args.client_id();
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let remote_connection_id = old_connection_cell
        .get_by_id(&connection_args.client_id(), connection_id.as_str())
        .ok_or_else(|| Error::connection_not_found(connection_id.clone()))?
        .counterparty
        .connection_id
        .clone();

    let ibc_channel_end = convert_channel_end(
        msg.channel.clone(),
        msg.port_id.clone(),
        remote_connection_id,
        next_channel_num,
    )?;
    let ibc_channel = get_encoded_object(&ibc_channel_end);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenInit,
        content: rlp::encode(&CkbMsgChannelOpenInit {}).to_vec(),
    };

    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: false,
        channel_id: next_channel_num,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let old_connection = get_encoded_object(&old_connection_cell);
    let new_connection = get_encoded_object(&new_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let (connection_input, input_capacity) = converter.get_ibc_connections_input(&client_id)?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .output(channel_lock, ibc_channel.data)
        .witness(old_connection.witness, new_connection.witness)
        .witness(BytesOpt::default(), ibc_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), &get_channel_id_str(next_channel_num));
    let channel_id = get_channel_id_str(next_channel_num);
    let event = IbcEvent::OpenInitChannel(OpenInit {
        port_id: msg.port_id,
        channel_id: Some(channel_id.parse().unwrap()),
        connection_id: msg.channel.connection_hops[0].clone(),
        counterparty_port_id: msg.channel.remote.port_id,
        counterparty_channel_id: msg.channel.remote.channel_id,
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_chan_open_try_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenTry,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let connection_id = &msg.channel.connection_hops[0];

    let (connection_args, old_connection_cell) =
        converter.get_ibc_connections_by_connection_id(&msg.channel.connection_hops[0])?;
    let client_id = connection_args.client_id();
    let next_channel_num = old_connection_cell.next_channel_number;
    let mut new_connection_cell = old_connection_cell.clone();
    new_connection_cell.next_channel_number += 1;

    let remote_connection_id = old_connection_cell
        .get_by_id(&connection_args.client_id(), connection_id.as_str())
        .ok_or_else(|| Error::connection_not_found(connection_id.clone()))?
        .counterparty
        .connection_id
        .clone();

    let ibc_channel_end = convert_channel_end(
        msg.channel.clone(),
        msg.port_id.clone(),
        remote_connection_id,
        next_channel_num,
    )?;
    let ibc_channel = get_encoded_object(&ibc_channel_end);

    let old_connection = get_encoded_object(&old_connection_cell);
    let new_connection = get_encoded_object(&new_connection_cell);

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenTry,
        content: rlp::encode(&CkbMsgChannelOpenTry {
            proof_height: convert_proof_height(msg.proofs.height()),
            proof_init: msg.proofs.object_proof().clone().into(),
        })
        .to_vec(),
    };

    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: false,
        channel_id: next_channel_num,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let (connection_input, input_capacity) = converter.get_ibc_connections_input(&client_id)?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .output(channel_lock, ibc_channel.data)
        .witness(old_connection.witness, new_connection.witness)
        .witness(BytesOpt::default(), ibc_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), &get_channel_id_str(next_channel_num));
    let channel_id = get_channel_id_str(next_channel_num);
    let event = IbcEvent::OpenTryChannel(OpenTry {
        port_id: msg.port_id,
        channel_id: Some(channel_id.parse().unwrap()),
        connection_id: msg.channel.connection_hops[0].clone(),
        counterparty_port_id: msg.channel.remote.port_id,
        counterparty_channel_id: msg.channel.remote.channel_id,
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_chan_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenAck,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let channel_idx = get_channel_number(&msg.channel_id)?;
    let old_channel = converter.get_ibc_channel(&msg.channel_id, Some(&msg.port_id))?;
    if old_channel.state == CkbState::Open {
        return Err(Error::other_error(format!(
            "channel {} has already opened",
            msg.channel_id
        )));
    }
    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id).unwrap();
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Open;
    new_channel.counterparty.channel_id = msg.counterparty_channel_id.to_string();
    new_channel.version = msg.counterparty_version.to_string();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenAck,
        content: rlp::encode(&CkbMsgChannelOpenAck {
            proof_height: convert_proof_height(msg.proofs.height()),
            proof_try: msg.proofs.object_proof().clone().into(),
        })
        .to_vec(),
    };

    let connection_id = old_channel.connection_hops[0].parse().unwrap();
    let (connection_args, _) = converter.get_ibc_connections_by_connection_id(&connection_id)?;
    let client_id = connection_args.client_id();
    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: true,
        channel_id: channel_idx,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let old_channel = get_encoded_object(&old_channel);
    let new_channel = get_encoded_object(&new_channel);
    let (channel_input, input_capacity) =
        converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id)?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(channel_input.clone())
        .output(channel_lock, new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), msg.channel_id.as_ref());
    let event = IbcEvent::OpenAckChannel(OpenAck {
        port_id: msg.port_id,
        channel_id: Some(msg.channel_id),
        connection_id,
        counterparty_port_id,
        counterparty_channel_id: Some(msg.counterparty_channel_id),
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_chan_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelOpenConfirm,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_channel = converter.get_ibc_channel(&msg.channel_id, Some(&msg.port_id))?;
    if old_channel.state == CkbState::Open {
        return Err(Error::other_error(format!(
            "channel {} has already opened",
            msg.channel_id
        )));
    }
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Open;

    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id)
        .map_err(|_| Error::ckb_port_id_invalid(old_channel.counterparty.port_id.clone()))?;
    let counterparty_channel_id =
        ChannelId::from_str(&old_channel.counterparty.channel_id).unwrap();

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelOpenConfirm,
        content: rlp::encode(&CkbMsgChannelOpenConfirm {
            proof_height: convert_proof_height(msg.proofs.height()),
            proof_ack: msg.proofs.object_proof().clone().into(),
        })
        .to_vec(),
    };

    let connection_id = old_channel.connection_hops[0].parse().unwrap();
    let (connection_args, _) = converter.get_ibc_connections_by_connection_id(&connection_id)?;
    let client_id = connection_args.client_id();
    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: true,
        channel_id: get_channel_number(&msg.channel_id)?,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let old_channel = get_encoded_object(&old_channel);
    let new_channel = get_encoded_object(&new_channel);
    let (channel_input, input_capacity) =
        converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id)?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(channel_input.clone())
        .output(channel_lock, new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), msg.channel_id.as_ref());
    let event = IbcEvent::OpenConfirmChannel(OpenConfirm {
        port_id: msg.port_id,
        channel_id: Some(msg.channel_id),
        connection_id,
        counterparty_port_id,
        counterparty_channel_id: Some(counterparty_channel_id),
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_chan_close_init_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelCloseInit,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_channel = converter.get_ibc_channel(&msg.channel_id, Some(&msg.port_id))?;
    if old_channel.state == CkbState::Closed {
        return Err(Error::other_error(format!(
            "channel {} has already closed",
            msg.channel_id
        )));
    }
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Closed;

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelCloseInit,
        content: rlp::encode(&CkbMsgChannelCloseInit {}).to_vec(),
    };

    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id)
        .map_err(|_| Error::ckb_port_id_invalid(old_channel.counterparty.port_id.clone()))?;
    let counterparty_channel_id =
        ChannelId::from_str(&old_channel.counterparty.channel_id).unwrap();

    let connection_id = ConnectionId::from_str(&old_channel.connection_hops[0])
        .map_err(|_| Error::ckb_conn_id_invalid(old_channel.connection_hops[0].clone()))?;
    let (connection_args, _) = converter.get_ibc_connections_by_connection_id(&connection_id)?;
    let client_id = connection_args.client_id();
    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: false,
        channel_id: new_channel.number,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let (channel_input, input_capacity) =
        converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id)?;

    let old_channel = get_encoded_object(&old_channel);
    let new_channel = get_encoded_object(&new_channel);

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(channel_input.clone())
        .output(channel_lock, new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), msg.channel_id.as_ref());
    let event = IbcEvent::CloseInitChannel(CloseInit {
        port_id: msg.port_id,
        channel_id: msg.channel_id,
        connection_id,
        counterparty_port_id,
        counterparty_channel_id: Some(counterparty_channel_id),
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_chan_close_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgChannelCloseConfirm,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_channel = converter.get_ibc_channel(&msg.channel_id, Some(&msg.port_id))?;
    if old_channel.state == CkbState::Closed {
        return Err(Error::other_error(format!(
            "channel {} has already closed",
            msg.channel_id
        )));
    }
    let mut new_channel = old_channel.clone();
    new_channel.state = CkbState::Closed;

    let envelope = Envelope {
        msg_type: MsgType::MsgChannelCloseConfirm,
        content: rlp::encode(&CkbMsgChannelCloseConfirm {
            proof_height: convert_proof_height(msg.proofs.height()),
            proof_init: msg.proofs.object_proof().clone().into(),
        })
        .to_vec(),
    };

    let counterparty_port_id = PortId::from_str(&old_channel.counterparty.port_id)
        .map_err(|_| Error::ckb_port_id_invalid(old_channel.counterparty.port_id.clone()))?;
    let counterparty_channel_id =
        ChannelId::from_str(&old_channel.counterparty.channel_id).unwrap();

    let connection_id = ConnectionId::from_str(&old_channel.connection_hops[0])
        .map_err(|_| Error::ckb_conn_id_invalid(old_channel.connection_hops[0].clone()))?;
    let (connection_args, _) = converter.get_ibc_connections_by_connection_id(&connection_id)?;
    let client_id = connection_args.client_id();
    let channel_args = ChannelArgs {
        metadata_type_id: connection_args.metadata_type_id,
        ibc_handler_address: connection_args.ibc_handler_address,
        open: false,
        channel_id: new_channel.number,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let (channel_input, input_capacity) =
        converter.get_ibc_channel_input(&msg.channel_id, &msg.port_id)?;

    let old_channel = get_encoded_object(&old_channel);
    let new_channel = get_encoded_object(&new_channel);

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(channel_input.clone())
        .output(channel_lock, new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
        .build();

    let commitment_path = channel_path(msg.port_id.as_ref(), msg.channel_id.as_ref());
    let event = IbcEvent::CloseConfirmChannel(CloseConfirm {
        port_id: msg.port_id,
        channel_id: Some(msg.channel_id),
        connection_id,
        counterparty_port_id,
        counterparty_channel_id: Some(counterparty_channel_id),
    });

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}
