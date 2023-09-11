use ckb_ics_axon::{
    handler::IbcConnections,
    message::{Envelope, MsgClientCreate, MsgType},
    ConnectionArgs,
};
use ckb_sdk::{
    traits::SecpCkbRawKeySigner,
    unlock::{ScriptSigner, SecpSighashScriptSigner},
    ScriptGroup, ScriptGroupType,
};
use ckb_types::{
    core::{Capacity, DepType, ScriptHashType, TransactionView},
    packed::{BytesOpt, CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::{Builder, Entity, Pack},
    H256,
};

use crate::generator::{
    calc_script_hash,
    utils::{get_lock_script, get_secp256k1_cell_dep, keccak256, wrap_rpc_request_and_save},
    PRIVKEY,
};

use super::{
    deploy_connection::ConnectionAttribute, deploy_packet_metadata::PacketMetataAttribute,
    deploy_sudt::SUDTAttribute,
};

pub fn generate_create_connection_sudt(
    sudt_attr: &SUDTAttribute,
    connection_attr: &ConnectionAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) -> (H256, usize) {
    let tx_hash = packet_metadata_attr.tx_hash.clone();
    let change_idx: usize = packet_metadata_attr.balance_index;
    let metadata_idx: usize = packet_metadata_attr.metadata_index;
    let connection_idx: usize = connection_attr.connection_index;
    let connection_code_hash = connection_attr.connection_code_hash.clone();
    let metadata_code_hash = packet_metadata_attr.metadata_code_hash.clone();
    let metadata_args = packet_metadata_attr.metadata_type_args.as_bytes();

    let metadata_dep = CellDep::new_builder()
        .dep_type(DepType::Code.into())
        .out_point(
            OutPoint::new_builder()
                .tx_hash(tx_hash.pack())
                .index(metadata_idx.pack())
                .build(),
        )
        .build();

    let connection_dep = CellDep::new_builder()
        .dep_type(DepType::Code.into())
        .out_point(
            OutPoint::new_builder()
                .tx_hash(tx_hash.pack())
                .index(connection_idx.pack())
                .build(),
        )
        .build();

    let sudt_dep = CellDep::new_builder()
        .dep_type(DepType::Code.into())
        .out_point(
            OutPoint::new_builder()
                .tx_hash(sudt_attr.tx_hash.pack())
                .index(sudt_attr.sudt_index.pack())
                .build(),
        )
        .build();

    let secp256k1_dep = get_secp256k1_cell_dep();
    let input = CellInput::new_builder()
        .previous_output(
            OutPoint::new_builder()
                .tx_hash(tx_hash.pack())
                .index(change_idx.pack())
                .build(),
        )
        .build();
    let connection_lock_args = ConnectionArgs {
        client_id: calc_script_hash(&metadata_code_hash, metadata_args).into(),
    }
    .client_id
    .as_slice()
    .pack();
    println!("connection lock args: {:?}", connection_lock_args);

    let connection = IbcConnections::default();
    let connection_data = rlp::encode(&connection);
    let hash = keccak256(&connection_data).as_slice().pack();
    let connection_output = CellOutput::new_builder()
        .lock(
            Script::new_builder()
                .hash_type(ScriptHashType::Type.into())
                .code_hash(connection_code_hash.pack())
                .args(connection_lock_args)
                .build(),
        )
        .build_exact_capacity(Capacity::bytes(hash.len()).unwrap())
        .unwrap();

    let (lock_script, secret_key, _) = get_lock_script(PRIVKEY);
    let issue_sudt_output = CellOutput::new_builder()
        .lock(lock_script.clone())
        .type_(
            Some(
                Script::new_builder()
                    .code_hash(sudt_attr.sudt_code_hash.pack())
                    .hash_type(ScriptHashType::Type.into())
                    .args(lock_script.calc_script_hash().as_bytes().pack())
                    .build(),
            )
            .pack(),
        )
        .build_exact_capacity(Capacity::bytes(u128::BITS as usize / 8).unwrap())
        .unwrap();
    let usdt_data = 10000u128.to_le_bytes().to_vec().pack();

    let change_output = CellOutput::new_builder()
        .lock(lock_script.clone())
        .capacity(400_000_000_000_000u64.pack())
        .build();
    let empty_data = "".as_bytes().to_vec().pack();

    let envelope = Envelope {
        msg_type: MsgType::MsgClientCreate,
        content: rlp::encode(&MsgClientCreate {}).to_vec(),
    };
    let envelope_witness = WitnessArgs::new_builder()
        .output_type(
            BytesOpt::new_builder()
                .set(Some(rlp::encode(&envelope).to_vec().pack()))
                .build(),
        )
        .build();

    let tx = TransactionView::new_advanced_builder()
        .cell_dep(metadata_dep)
        .cell_dep(connection_dep)
        .cell_dep(secp256k1_dep)
        .cell_dep(sudt_dep)
        .input(input)
        .output(connection_output)
        .output(issue_sudt_output)
        .output(change_output)
        .output_data(hash)
        .output_data(usdt_data)
        .output_data(empty_data)
        .witness(
            WitnessArgs::new_builder()
                .output_type(
                    BytesOpt::new_builder()
                        .set(Some(connection_data.pack()))
                        .build(),
                )
                .build()
                .as_slice()
                .pack(),
        )
        .witness(WitnessArgs::default().as_slice().pack())
        .witness(envelope_witness.as_slice().pack())
        .build();
    println!("envelope slice: {:?}", rlp::encode(&envelope).to_vec());
    println!("witness args slice: {:?}", envelope_witness.as_slice());
    let signer =
        SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![
            secret_key,
        ])));
    let tx = signer
        .sign_tx(
            &tx,
            &ScriptGroup {
                script: lock_script,
                group_type: ScriptGroupType::Lock,
                input_indices: vec![0],
                output_indices: vec![2],
            },
        )
        .unwrap();
    let tx_hash = wrap_rpc_request_and_save(tx, "./txs/create_connection_sudt.json");
    (tx_hash, 2)
}
