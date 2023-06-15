use crate::{
    chain::ckb4ibc::utils::{convert_proof, get_connection_idx, get_encoded_object},
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
use ckb_types::{
    core::{Capacity, DepType, TransactionView},
    packed::{CellDep, CellOutput, Script, WitnessArgs},
    prelude::{Builder, Entity, Pack},
};
use ibc_relayer_types::core::ics03_connection::msgs::{
    conn_open_ack::MsgConnectionOpenAck, conn_open_confirm::MsgConnectionOpenConfirm,
    conn_open_init::MsgConnectionOpenInit, conn_open_try::MsgConnectionOpenTry,
};

use super::MsgToTxConverter;

pub fn convert_conn_open_init_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenInit,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let client_id = msg.client_id.to_string();

    let remote_client_id = msg.counterparty.client_id().to_string();
    let counterparty = ConnectionCounterparty {
        client_id: remote_client_id,
        connection_id: None,
    };

    let connection_end = CkbConnectionEnd {
        state: State::Init,
        client_id,
        counterparty,
        delay_period: msg.delay_period.as_secs(),
    };
    let old_ibc_connection_cell = converter.get_ibc_connections();
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();
    new_ibc_connection_cell.connections.push(connection_end);
    new_ibc_connection_cell.next_connection_number += 1;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenInit,
        content: rlp::encode(&CkbMsgConnectionOpenInit {}).to_vec(),
    };

    let old_connection_encoded = get_encoded_object(old_ibc_connection_cell);
    let new_connection_encoded = get_encoded_object(new_ibc_connection_cell);

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
        .output_data(new_connection_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}

pub fn convert_conn_open_try_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenTry,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let client_id = msg.client_id.to_string();

    let remote_client_id = msg.counterparty.client_id().to_string();

    let remote_conn_id = msg.counterparty.connection_id.unwrap();
    let remote_conn_id = remote_conn_id.to_string();

    let counterparty = ConnectionCounterparty {
        client_id: remote_client_id,
        connection_id: Some(remote_conn_id),
    };

    let connection_end = CkbConnectionEnd {
        state: State::OpenTry,
        client_id,
        counterparty,
        delay_period: msg.delay_period.as_secs(),
    };
    let old_ibc_connection_cell = converter.get_ibc_connections();
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

    let old_connection_encoded = get_encoded_object(old_ibc_connection_cell);
    let new_connection_encoded = get_encoded_object(new_ibc_connection_cell);

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
        .output_data(new_connection_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}

pub fn convert_conn_open_ack_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenAck,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let old_ibc_connection_cell = converter.get_ibc_connections();
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_idx(&msg.connection_id)? as usize;
    let mut connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenAck,
        content: rlp::encode(&CkbMsgConnectionOpenAck {
            conn_id_on_a: idx,
            proof_conn_end_on_b: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };
    let old_connection_encoded = get_encoded_object(old_ibc_connection_cell);
    let new_connection_encoded = get_encoded_object(new_ibc_connection_cell);

    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_client_outpoint())
                .build(),
        )
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
        .output_data(new_connection_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}

pub fn convert_conn_open_confirm_to_tx<C: MsgToTxConverter>(
    msg: MsgConnectionOpenConfirm,
    converter: &C,
) -> Result<(TransactionView, Envelope, u64), Error> {
    let old_ibc_connection_cell = converter.get_ibc_connections();
    let mut new_ibc_connection_cell = old_ibc_connection_cell.clone();

    let idx = get_connection_idx(&msg.connection_id)? as usize;
    let mut connection_end = new_ibc_connection_cell.connections.get_mut(idx).unwrap();
    connection_end.state = State::Open;

    let envelope = Envelope {
        msg_type: MsgType::MsgConnectionOpenConfirm,
        content: rlp::encode(&CkbMsgConnectionOpenConfirm {
            conn_id_on_b: idx,
            proofs: convert_proof(msg.proofs)?,
        })
        .to_vec(),
    };
    let old_connection_encoded = get_encoded_object(old_ibc_connection_cell);
    let new_connection_encoded = get_encoded_object(new_ibc_connection_cell);

    let data_capacity = Capacity::bytes(32).map_err(|_| Error::capacity())?;

    let packed_tx = TransactionView::new_advanced_builder()
        .cell_dep(
            CellDep::new_builder()
                .dep_type(DepType::Code.into())
                .out_point(converter.get_client_outpoint())
                .build(),
        )
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
        .output_data(new_connection_encoded.data)
        .witness(
            WitnessArgs::new_builder()
                .input_type(old_connection_encoded.witness)
                .output_type(new_connection_encoded.witness)
                .build()
                .as_bytes()
                .pack(),
        )
        .build();
    Ok((packed_tx, envelope, CONNECTION_CELL_CAPACITY))
}
