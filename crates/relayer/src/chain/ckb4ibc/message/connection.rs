use ckb_ics_axon::{
    commitment::connection_path,
    connection_id,
    handler::{
        handle_msg_connection_open_ack, handle_msg_connection_open_confirm,
        handle_msg_connection_open_init, handle_msg_connection_open_try,
    },
    message::{
        Envelope, MsgConnectionOpenAck as CkbMsgConnectionOpenAck,
        MsgConnectionOpenConfirm as CkbMsgConnectionOpenConfirm,
        MsgConnectionOpenInit as CkbMsgConnectionOpenInit,
        MsgConnectionOpenTry as CkbMsgConnectionOpenTry, MsgType,
    },
    object::{ConnectionCounterparty, ConnectionEnd as CkbConnectionEnd, State},
    ConnectionArgs,
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

use super::{convert_proof_height, CkbTxInfo, EmptyClient, MsgToTxConverter, TxBuilder};
use crate::{
    chain::ckb4ibc::utils::{
        get_client_outpoint, get_connection_index_by_id, get_connection_lock_script,
        get_encoded_object,
    },
    error::Error,
};

pub fn convert_conn_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenInit,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let client_id = msg.client_id.to_string();

    let remote_client_id = msg.counterparty.client_id().to_string();
    let counterparty = ConnectionCounterparty {
        client_id: remote_client_id,
        connection_id: "".into(),
        commitment_prefix: converter.get_commitment_prefix(),
    };

    let connection_end = CkbConnectionEnd {
        state: State::Init,
        counterparty,
        delay_period: msg.delay_period.as_secs(),
        ..Default::default()
    };
    let old_ibc_connection_cell = converter.get_ibc_connections(&client_id)?;
    let this_conn_idx = old_ibc_connection_cell.connections.len();
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();
    new_ibc_connection_cell.connections.push(connection_end);

    let old_connection = get_encoded_object(&old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let (connection_input, input_capacity, old_connection_args) =
        converter.get_ibc_connections_input(&client_id)?;

    let mut commitments = vec![];
    let new_connection_args =
        ConnectionArgs::from_slice(&connection_lock.args().raw_data()).expect("connection args");
    handle_msg_connection_open_init(
        old_ibc_connection_cell,
        old_connection_args,
        new_ibc_connection_cell,
        new_connection_args,
        &mut commitments,
    )
    .map_err(|err| Error::other_error(format!("handle error: {}", err as i8)))?;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenInit,
        content: rlp::encode(&CkbMsgConnectionOpenInit {}).to_vec(),
        commitments,
    };

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let connection_id = connection_id(&client_id, this_conn_idx);
    let event = IbcEvent::OpenInitConnection(OpenInit(Attributes {
        connection_id: Some(connection_id.parse().unwrap()),
        client_id: msg.client_id,
        counterparty_connection_id: None,
        counterparty_client_id: msg.counterparty.client_id().clone(),
    }));
    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path: connection_path(&connection_id),
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
        connection_id: remote_conn_id,
        commitment_prefix: converter.get_commitment_prefix(),
    };

    let connection_end = CkbConnectionEnd {
        state: State::OpenTry,
        counterparty,
        delay_period: msg.delay_period.as_secs(),
        ..Default::default()
    };
    let old_ibc_connection_cell = converter.get_ibc_connections(&client_id)?;
    let this_conn_idx = old_ibc_connection_cell.connections.len();
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();
    new_ibc_connection_cell.connections.push(connection_end);

    let old_connection = get_encoded_object(&old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let (connection_input, input_capacity, old_connection_args) =
        converter.get_ibc_connections_input(&client_id)?;

    let ckb_msg = CkbMsgConnectionOpenTry {
        proof_height: convert_proof_height(msg.proofs.height()),
        proof_init: msg.proofs.object_proof().clone().into(),
    };
    let content = rlp::encode(&ckb_msg).to_vec();
    let new_connection_args =
        ConnectionArgs::from_slice(&connection_lock.args().raw_data()).expect("connection args");
    let mut commitments = vec![];
    handle_msg_connection_open_try(
        EmptyClient,
        old_ibc_connection_cell,
        old_connection_args,
        new_ibc_connection_cell,
        new_connection_args,
        &mut commitments,
        ckb_msg,
    )
    .map_err(|err| Error::other_error(format!("handle error: {}", err as i8)))?;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenTry,
        content,
        commitments,
    };

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let connection_id = connection_id(&client_id, this_conn_idx);
    let event = IbcEvent::OpenTryConnection(OpenTry(Attributes {
        connection_id: Some(connection_id.parse().unwrap()),
        client_id: msg.client_id,
        counterparty_connection_id: msg.counterparty.connection_id.clone(),
        counterparty_client_id: msg.counterparty.client_id().clone(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path: connection_path(&connection_id),
    })
}

pub fn convert_conn_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenAck,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let (old_connection_args, old_ibc_connection_cell) =
        converter.get_ibc_connections_by_connection_id(&msg.connection_id)?;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_index_by_id(&msg.connection_id)? as usize;
    let connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;
    connection_end.counterparty.connection_id = msg.counterparty_connection_id.to_string();

    let counterparty_client_id = connection_end.counterparty.client_id.clone();
    let client_id = old_connection_args.client_id();

    let old_connection = get_encoded_object(&old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let (connection_input, input_capacity, _) = converter.get_ibc_connections_input(&client_id)?;

    let ckb_msg = CkbMsgConnectionOpenAck {
        conn_id_on_a: idx,
        proof_height: convert_proof_height(msg.proofs.height()),
        proof_try: msg.proofs.object_proof().clone().into(),
    };
    let content = rlp::encode(&ckb_msg).to_vec();
    let new_connection_args =
        ConnectionArgs::from_slice(&connection_lock.args().raw_data()).expect("connection args");
    let mut commitments = vec![];
    handle_msg_connection_open_ack(
        EmptyClient,
        old_ibc_connection_cell,
        old_connection_args,
        new_ibc_connection_cell,
        new_connection_args,
        &mut commitments,
        ckb_msg,
    )
    .map_err(|err| Error::other_error(format!("handle error: {}", err as i8)))?;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenAck,
        content,
        commitments,
    };

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let commitment_path = connection_path(&msg.connection_id.to_string());
    let event = IbcEvent::OpenAckConnection(OpenAck(Attributes {
        connection_id: Some(msg.connection_id),
        client_id: client_id.parse().unwrap(),
        counterparty_connection_id: Some(msg.counterparty_connection_id),
        counterparty_client_id: counterparty_client_id.parse().unwrap(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}

pub fn convert_conn_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenConfirm,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let (old_connection_args, old_ibc_connection_cell) =
        converter.get_ibc_connections_by_connection_id(&msg.connection_id)?;
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_index_by_id(&msg.connection_id)? as usize;
    let connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;

    let counterparty_client_id = connection_end.counterparty.client_id.clone();
    let counterparty_connection_id = connection_end.counterparty.connection_id.parse().unwrap();
    let client_id = old_connection_args.client_id();

    let old_connection = get_encoded_object(&old_ibc_connection_cell);
    let new_connection = get_encoded_object(&new_ibc_connection_cell);
    let connection_lock =
        get_connection_lock_script(converter.get_config(), Some(client_id.clone()))?;
    let (connection_input, input_capacity, _) = converter.get_ibc_connections_input(&client_id)?;

    let ckb_msg = CkbMsgConnectionOpenConfirm {
        conn_id_on_b: idx,
        proof_height: convert_proof_height(msg.proofs.height()),
        proof_ack: msg.proofs.object_proof().clone().into(),
    };
    let content = rlp::encode(&ckb_msg).to_vec();
    let new_connection_args =
        ConnectionArgs::from_slice(&connection_lock.args().raw_data()).expect("connection args");
    let mut commitments = vec![];
    handle_msg_connection_open_confirm(
        EmptyClient,
        old_ibc_connection_cell,
        old_connection_args,
        new_ibc_connection_cell,
        new_connection_args,
        &mut commitments,
        ckb_msg,
    )
    .map_err(|err| Error::other_error(format!("handle error: {}", err as i8)))?;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenConfirm,
        content,
        commitments,
    };

    let packed_tx = TxBuilder::default()
        .cell_dep(get_client_outpoint(converter, &client_id)?)
        .cell_dep(converter.get_conn_contract_outpoint().clone())
        .input(connection_input.clone())
        .output(connection_lock, new_connection.data)
        .witness(old_connection.witness, new_connection.witness)
        .build();

    let commitment_path = connection_path(&msg.connection_id.to_string());
    let event = IbcEvent::OpenConfirmConnection(OpenConfirm(Attributes {
        connection_id: Some(msg.connection_id),
        client_id: client_id.parse().unwrap(),
        counterparty_connection_id: Some(counterparty_connection_id),
        counterparty_client_id: counterparty_client_id.parse().unwrap(),
    }));

    Ok(CkbTxInfo {
        unsigned_tx: Some(packed_tx),
        envelope,
        input_capacity,
        event: Some(event),
        commitment_path,
    })
}
