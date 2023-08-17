use ckb_hash::new_blake2b;
use ckb_sdk::{
    traits::SecpCkbRawKeySigner,
    unlock::{ScriptSigner, SecpSighashScriptSigner},
    ScriptGroup, ScriptGroupType,
};
use ckb_types::{
    core::{ScriptHashType, TransactionView},
    h256,
    packed::{CellInput, CellOutput, OutPoint, Script, ScriptOpt},
    prelude::*,
    H256,
};

use crate::generator::{
    utils::{get_lock_script, get_secp256k1_cell_dep, wrap_rpc_request_and_save},
    PRIVKEY,
};

use super::deploy_connection::ConnectionAttribute;

#[derive(Debug)]
pub struct ChannelAttribute {
    pub tx_hash: H256,
    pub balance_index: usize,
    pub channel_index: usize,
    pub channel_type_args: H256,
    pub channel_code_hash: H256,
}

pub fn generate_deploy_channel(attribute: &ConnectionAttribute) -> ChannelAttribute {
    let input = CellInput::new_builder()
        .previous_output(
            OutPoint::new_builder()
                .tx_hash(attribute.tx_hash.pack())
                .index(attribute.balance_index.pack())
                .build(),
        )
        .build();

    let mut blake_2b = new_blake2b();
    blake_2b.update(input.as_slice());
    blake_2b.update(0u64.to_le_bytes().as_slice());
    let mut type_0_args = [0; 32];
    blake_2b.finalize(&mut type_0_args);
    println!("channel type args: {:?}", hex::encode(type_0_args));
    let channel_type_args: H256 = type_0_args.into();

    let (lock_script, secret_key) = get_lock_script(PRIVKEY);

    let channel_type_script = Script::new_builder()
        .code_hash(
            h256!("0x00000000000000000000000000000000000000000000000000545950455f4944").pack(),
        )
        .hash_type(ScriptHashType::Type.into())
        .args(type_0_args.as_slice().pack())
        .build();

    println!(
        "channel code hash: {}",
        channel_type_script.calc_script_hash()
    );
    let channel_code_hash: H256 = channel_type_script.calc_script_hash().unpack();

    let channel_output = CellOutput::new_builder()
        .type_(
            ScriptOpt::new_builder()
                .set(Some(channel_type_script))
                .build(),
        )
        .lock(lock_script.clone())
        .capacity(100_000_000_000_000u64.pack())
        .build();

    let change_output = CellOutput::new_builder()
        .lock(lock_script.clone())
        .capacity(800_000_000_000_000_u64.pack())
        .build();
    let empty_data = "0x".as_bytes().to_vec().pack();

    let tx = TransactionView::new_advanced_builder()
        .cell_dep(get_secp256k1_cell_dep())
        .input(input)
        .output(channel_output)
        .output(change_output)
        .output_data(std::fs::read("./contracts/ics-channel").unwrap().pack())
        .output_data(empty_data)
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
    let tx_hash = wrap_rpc_request_and_save(tx, "./txs/deploy_channel.json");

    ChannelAttribute {
        tx_hash,
        balance_index: 1,
        channel_index: 0,
        channel_type_args,
        channel_code_hash,
    }
}
