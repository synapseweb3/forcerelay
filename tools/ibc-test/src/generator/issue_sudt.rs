use ckb_sdk::{
    traits::SecpCkbRawKeySigner,
    unlock::{ScriptSigner, SecpSighashScriptSigner},
    ScriptGroup, ScriptGroupType,
};
use ckb_types::{
    core::{Capacity, DepType, ScriptHashType, TransactionView},
    packed::{CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
    prelude::{Builder, Entity, Pack},
    H256,
};

use crate::generator::{
    utils::{get_lock_script, get_secp256k1_cell_dep, wrap_rpc_request_and_save},
    PRIVKEY,
};

use super::{deploy_packet_metadata::PacketMetataAttribute, deploy_sudt::SUDTAttribute};

pub fn generate_issue_sudt(
    sudt_attr: &SUDTAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) -> (H256, usize) {
    let tx_hash = packet_metadata_attr.tx_hash.clone();
    let change_idx: usize = packet_metadata_attr.balance_index;

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

    let tx = TransactionView::new_advanced_builder()
        .cell_dep(secp256k1_dep)
        .cell_dep(sudt_dep)
        .input(input)
        .output(issue_sudt_output)
        .output(change_output)
        .output_data(usdt_data)
        .output_data(empty_data)
        .witness(WitnessArgs::default().as_slice().pack())
        .build();
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
                output_indices: vec![1],
            },
        )
        .unwrap();
    let tx_hash = wrap_rpc_request_and_save(tx, "./txs/issue_sudt.json");
    (tx_hash, 2)
}
