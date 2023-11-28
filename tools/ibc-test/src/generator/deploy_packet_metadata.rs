use axon_types::metadata::{Metadata, MetadataCellData, MetadataList, Validator, ValidatorList};
use ckb_hash::new_blake2b;
use ckb_sdk::{
    constants::TYPE_ID_CODE_HASH,
    traits::SecpCkbRawKeySigner,
    unlock::{ScriptSigner, SecpSighashScriptSigner},
    ScriptGroup, ScriptGroupType,
};
use ckb_types::{
    core::{Capacity, ScriptHashType, TransactionView},
    packed::{CellInput, CellOutput, OutPoint, Script, ScriptOpt},
    prelude::*,
    H256,
};

use super::deploy_channel::ChannelAttribute;
use crate::generator::{
    utils::{get_lock_script, get_secp256k1_cell_dep, wrap_rpc_request_and_save},
    PRIVKEY,
};

pub struct PacketMetataAttribute {
    pub tx_hash: H256,
    pub packet_type_args: H256,
    pub packet_code_hash: H256,
    pub packet_index: usize,
    pub metadata_index: usize,
    pub metadata_code_hash: H256,
    pub metadata_type_args: H256,
    pub balance_index: usize,
}

pub fn generate_deploy_packet_metadata(attribute: &ChannelAttribute) -> PacketMetataAttribute {
    let input = CellInput::new_builder()
        .previous_output(
            OutPoint::new_builder()
                .tx_hash(attribute.tx_hash.pack())
                .index(attribute.balance_index.pack())
                .build(),
        )
        .build();

    let (lock_script, secret_key, _) = get_lock_script(PRIVKEY);

    let mut blake_2b = new_blake2b();
    blake_2b.update(input.as_slice());
    blake_2b.update(0u64.to_le_bytes().as_slice());
    let mut type_0_args = [0; 32];
    blake_2b.finalize(&mut type_0_args);
    println!("packet type args: {:?}", hex::encode(type_0_args));
    let packet_type_args: H256 = type_0_args.into();

    let mut blake_2b = new_blake2b();
    blake_2b.update(input.as_slice());
    blake_2b.update(1u64.to_le_bytes().as_slice());
    let mut type_1_args = [0; 32];
    blake_2b.finalize(&mut type_1_args);
    println!("client type args: {:?}", hex::encode(type_1_args));
    let metadata_type_args: H256 = type_1_args.into();
    // let metadata_type_args: H256 = type_2_args.into();

    let packet_type_script = Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(type_0_args.as_slice().pack())
        .build();

    println!(
        "packet code hash: {}",
        packet_type_script.calc_script_hash()
    );
    let packet_code_hash: H256 = packet_type_script.calc_script_hash().unpack();

    let packet_output = CellOutput::new_builder()
        .type_(
            ScriptOpt::new_builder()
                .set(Some(packet_type_script))
                .build(),
        )
        .lock(lock_script.clone())
        .capacity(200_000_000_000_000u64.pack())
        .build();

    let mock_metadata_script = Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(type_1_args.as_slice().pack())
        .build();

    // Same as axon example single node spec which is used in ibc-tests.
    let bls_pub_key = hex::decode("a26e3fe1cf51bd4822072c61bdc315ac32e3d3c2e2484bb92942666399e863b4bf56cf2926383cc706ffc15dfebc85c6").unwrap();
    let metadata = Metadata::new_builder()
        .validators(
            ValidatorList::new_builder()
                .push(
                    Validator::new_builder()
                        // Only bls_pub_key matters for now.
                        .bls_pub_key(Entity::from_slice(&bls_pub_key).unwrap())
                        .build(),
                )
                .build(),
        )
        .build();

    let metadata_cell_data = MetadataCellData::new_builder()
        .metadata(MetadataList::new_builder().push(metadata).build())
        .build();

    let metadata_output = CellOutput::new_builder()
        .lock(lock_script.clone())
        .type_(
            ScriptOpt::new_builder()
                .set(Some(mock_metadata_script))
                .build(),
        )
        .build_exact_capacity(Capacity::bytes(metadata_cell_data.as_bytes().len()).unwrap())
        .unwrap();

    let change_output = CellOutput::new_builder()
        .lock(lock_script.clone())
        .capacity(500_000_000_000_000u64.pack())
        .build();

    let signer =
        SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![
            secret_key,
        ])));
    let empty_data = "0x".as_bytes().to_vec().pack();
    let tx = TransactionView::new_advanced_builder()
        .cell_dep(get_secp256k1_cell_dep())
        .input(input)
        .output(packet_output)
        .output(metadata_output)
        .output(change_output)
        .output_data(std::fs::read("./contracts/ics-packet").unwrap().pack())
        .output_data(metadata_cell_data.as_bytes().pack())
        .output_data(empty_data)
        .build();

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

    let tx_hash = wrap_rpc_request_and_save(tx, "./txs/deploy_packet_metadata.json");

    PacketMetataAttribute {
        tx_hash,
        packet_type_args,
        packet_code_hash,
        metadata_code_hash: TYPE_ID_CODE_HASH.clone(),
        metadata_type_args,
        packet_index: 0,
        metadata_index: 1,
        balance_index: 2,
    }
}
