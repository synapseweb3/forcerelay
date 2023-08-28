use ckb_ics_axon::consts::{CHANNEL_CELL_CAPACITY, CONNECTION_CELL_CAPACITY};
use ckb_ics_axon::handler::{IbcChannel, Sequence};
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::message::MsgChannelOpenAck as CkbMsgChannelOpenAck;
use ckb_ics_axon::message::MsgChannelOpenConfirm as CkbMsgChannelOpenConfirm;
use ckb_ics_axon::message::MsgChannelOpenInit as CkbMsgChannelOpenInit;
use ckb_ics_axon::message::MsgChannelOpenTry as CkbMsgChannelOpenTry;
use ckb_ics_axon::message::MsgType;
use ckb_ics_axon::object::{ChannelCounterparty, Ordering as CkbOrdering, State as CkbState};
use ckb_ics_axon::ChannelArgs;
use ckb_types::packed::BytesOpt;
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, Order, State};
use ibc_relayer_types::core::ics04_channel::events::{OpenAck, OpenConfirm, OpenInit, OpenTry};
use ibc_relayer_types::core::ics04_channel::msgs::{
    chan_close_confirm::MsgChannelCloseConfirm, chan_close_init::MsgChannelCloseInit,
    chan_open_ack::MsgChannelOpenAck, chan_open_confirm::MsgChannelOpenConfirm,
    chan_open_init::MsgChannelOpenInit, chan_open_try::MsgChannelOpenTry,
};
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, PortId};
use ibc_relayer_types::events::IbcEvent;
use std::str::FromStr;

use super::{CkbTxInfo, MsgToTxConverter, TxBuilder};
use crate::chain::ckb4ibc::utils::{
    convert_port_id_to_array, convert_proof, extract_client_id_by_connection_id,
    generate_channel_id, get_channel_capacity, get_channel_lock_script, get_channel_number,
    get_client_id_from_channel, get_client_outpoint, get_connection_capacity,
    get_connection_lock_script, get_encoded_object,
};
use crate::error::Error;

fn convert_channel_end(
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
    };
    Ok(result)
}

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
    let ibc_channel = get_encoded_object(&ibc_channel_end);

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

    let old_connection = get_encoded_object(old_connection_cell);
    let new_connection = get_encoded_object(&new_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .output(channel_lock, get_channel_capacity(), ibc_channel.data)
        .witness(old_connection.witness, new_connection.witness)
        .witness(BytesOpt::default(), ibc_channel.witness)
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
    let ibc_channel = get_encoded_object(&ibc_channel_end);

    let (client_cell_type_args, client_id) = get_client_id_from_channel(&msg.channel, converter)?;
    let old_connection = get_encoded_object(old_connection_cell);
    let new_connection = get_encoded_object(&new_connection_cell);

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
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .output(channel_lock, get_channel_capacity(), ibc_channel.data)
        .witness(old_connection.witness, new_connection.witness)
        .witness(BytesOpt::default(), ibc_channel.witness)
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
    let channel_idx = get_channel_number(&msg.channel_id)?;
    let old_channel = converter.get_ibc_channel(&msg.channel_id)?;
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

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let old_channel = get_encoded_object(old_channel);
    let new_channel = get_encoded_object(&new_channel);

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(
            converter
                .get_ibc_channel_input(&msg.channel_id, &msg.port_id)?
                .clone(),
        )
        .output(channel_lock, get_channel_capacity(), new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
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
    let old_channel = converter.get_ibc_channel(&msg.channel_id)?;
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
    let channel_args = ChannelArgs {
        client_id: client_cell_type_args,
        open: true,
        channel_id: get_channel_number(&msg.channel_id)?,
        port_id: convert_port_id_to_array(&msg.port_id)?,
    };

    let channel_lock = get_channel_lock_script(converter, channel_args.to_args());
    let old_channel = get_encoded_object(old_channel);
    let new_channel = get_encoded_object(&new_channel);

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_chan_contract_outpoint().clone())
        .input(
            converter
                .get_ibc_channel_input(&msg.channel_id, &msg.port_id)?
                .clone(),
        )
        .output(channel_lock, get_channel_capacity(), new_channel.data)
        .witness(old_channel.witness, new_channel.witness)
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

pub fn convert_chan_close_confirm_to_tx<C: MsgToTxConverter>(
    _msg: MsgChannelCloseConfirm,
    _converter: &C,
) -> Result<CkbTxInfo, Error> {
    todo!()
}
