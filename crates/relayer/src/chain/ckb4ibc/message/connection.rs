use crate::{
    chain::ckb4ibc::utils::{
        convert_proof, generate_connection_id, get_client_outpoint, get_connection_capacity,
        get_connection_index_by_id, get_connection_lock_script, get_encoded_object,
    },
    error::Error,
};
use ckb_ics_axon::consts::CONNECTION_CELL_CAPACITY;
use ckb_ics_axon::{
    message::{
        Envelope, MsgConnectionOpenAck as CkbMsgConnectionOpenAck,
        MsgConnectionOpenConfirm as CkbMsgConnectionOpenConfirm,
        MsgConnectionOpenInit as CkbMsgConnectionOpenInit,
        MsgConnectionOpenTry as CkbMsgConnectionOpenTry, MsgType,
    },
    object::{ConnectionCounterparty, ConnectionEnd as CkbConnectionEnd, State},
};
use ibc_relayer_types::{
    core::ics03_connection::{
        events::{Attributes, OpenAck, OpenConfirm, OpenInit, OpenTry},
        msgs::{
            conn_open_ack::MsgConnectionOpenAck, conn_open_confirm::MsgConnectionOpenConfirm,
            conn_open_init::MsgConnectionOpenInit, conn_open_try::MsgConnectionOpenTry,
        },
    },
    events::IbcEvent,
};

use super::{CkbTxInfo, MsgToTxConverter, TxBuilder};

pub fn convert_conn_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenInit,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let client_id = msg.client_id.to_string();

    let remote_client_id = msg.counterparty.client_id().to_string();
    let counterparty = ConnectionCounterparty {
        client_id: remote_client_id,
        connection_id: None,
        commitment_prefix: converter.get_commitment_prefix(),
    };

    let connection_end = CkbConnectionEnd {
        state: State::Init,
        client_id: client_id.clone(),
        counterparty,
        delay_period: msg.delay_period.as_secs(),
        versions: vec![Default::default()],
    };
    let old_ibc_connection_cell = converter.get_ibc_connections(&client_id)?;
    let this_conn_idx = old_ibc_connection_cell.next_connection_number;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();
    new_ibc_connection_cell.connections.push(connection_end);
    new_ibc_connection_cell.next_connection_number += 1;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenInit,
        content: rlp::encode(&CkbMsgConnectionOpenInit {}).to_vec(),
    };

    let old_connection = get_encoded_object(old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let event = IbcEvent::OpenInitConnection(OpenInit(Attributes {
        connection_id: Some(generate_connection_id(this_conn_idx, &client_id)),
        client_id: msg.client_id,
        counterparty_connection_id: None,
        counterparty_client_id: msg.counterparty.client_id().clone(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_conn_open_try_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenTry,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let client_id = msg.client_id.to_string();
    let remote_client_id = msg.counterparty.client_id().to_string();

    let remote_conn_id = msg.counterparty.connection_id.clone().unwrap();
    let remote_conn_id = remote_conn_id.to_string();

    let counterparty = ConnectionCounterparty {
        client_id: remote_client_id,
        connection_id: Some(remote_conn_id),
        commitment_prefix: converter.get_commitment_prefix(),
    };

    let connection_end = CkbConnectionEnd {
        state: State::OpenTry,
        client_id: client_id.clone(),
        counterparty,
        delay_period: msg.delay_period.as_secs(),
        versions: vec![Default::default()],
    };
    let old_ibc_connection_cell = converter.get_ibc_connections(&client_id)?;
    let this_conn_idx = old_ibc_connection_cell.next_connection_number;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();
    new_ibc_connection_cell.connections.push(connection_end);
    new_ibc_connection_cell.next_connection_number += 1;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenTry,
        content: rlp::encode(&CkbMsgConnectionOpenTry {
            proof: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let old_connection = get_encoded_object(old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let event = IbcEvent::OpenTryConnection(OpenTry(Attributes {
        connection_id: Some(generate_connection_id(this_conn_idx, &client_id)),
        client_id: msg.client_id,
        counterparty_connection_id: msg.counterparty.connection_id.clone(),
        counterparty_client_id: msg.counterparty.client_id().clone(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_conn_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenAck,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_ibc_connection_cell =
        converter.get_ibc_connections_by_connection_id(&msg.connection_id)?;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_index_by_id(&msg.connection_id)? as usize;
    let connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;
    connection_end.counterparty.connection_id = Some(msg.counterparty_connection_id.to_string());

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenAck,
        content: rlp::encode(&CkbMsgConnectionOpenAck {
            conn_id_on_a: idx,
            proof_conn_end_on_b: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let counterparty_client_id = connection_end.counterparty.client_id.clone();
    let client_id = connection_end.client_id.clone();

    let old_connection = get_encoded_object(old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let event = IbcEvent::OpenAckConnection(OpenAck(Attributes {
        connection_id: Some(msg.connection_id),
        client_id: client_id.parse().unwrap(),
        counterparty_connection_id: Some(msg.counterparty_connection_id),
        counterparty_client_id: counterparty_client_id.parse().unwrap(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}

pub fn convert_conn_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenConfirm,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let old_ibc_connection_cell =
        converter.get_ibc_connections_by_connection_id(&msg.connection_id)?;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_index_by_id(&msg.connection_id)? as usize;
    let connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenConfirm,
        content: rlp::encode(&CkbMsgConnectionOpenConfirm {
            conn_id_on_b: idx,
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };

    let counterparty_client_id = connection_end.counterparty.client_id.clone();
    let counterparty_connection_id = connection_end
        .counterparty
        .connection_id
        .clone()
        .map(|v| v.parse().unwrap());
    let client_id = connection_end.client_id.clone();

    let old_connection = get_encoded_object(old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(converter.get_ibc_connections_input(&client_id)?.clone())
        .output(
            connection_lock,
            get_connection_capacity(),
            new_connection.data,
        )
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let event = IbcEvent::OpenConfirmConnection(OpenConfirm(Attributes {
        connection_id: Some(msg.connection_id),
        client_id: client_id.parse().unwrap(),
        counterparty_connection_id,
        counterparty_client_id: counterparty_client_id.parse().unwrap(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity: CONNECTION_CELL_CAPACITY,
        event: Some(event),
    })
}
