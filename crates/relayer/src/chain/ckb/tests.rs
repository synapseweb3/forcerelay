use std::{
    fs::{self, read_to_string},
    str::FromStr,
    sync::Arc,
};

use ckb_sdk::{
    constants::TYPE_ID_CODE_HASH,
    rpc::ckb_indexer::{Cell, SearchKey},
    traits::{CellQueryOptions, PrimaryScriptType},
    NetworkType,
};
use ckb_types::{
    core::{BlockNumber, Capacity, ScriptHashType},
    h256, packed,
    prelude::*,
};
use hdpath::StandardHDPath;
use ibc_relayer_types::{
    clients::ics07_eth::types::{Header as EthHeader, Update as EthUpdate},
    core::ics24_host::identifier::ChainId,
};
use rand::{thread_rng, Rng as _};
use tempfile::TempDir;
use tendermint_rpc::Url;
use tokio::runtime::Runtime as TokioRuntime;

use super::{CkbChain, HD_PATH};
use crate::{
    chain::endpoint::ChainEndpoint,
    config::{ckb::ChainConfig as CkbChainConfig, AddressType, ChainConfig},
    keyring::{Secp256k1KeyPair, SigningKeyPair},
};

const TESTDATA_DIR: &str = "src/testdata/test_update_eth_client";

fn random_hash() -> packed::Byte32 {
    let mut rng = thread_rng();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf);
    buf.pack()
}

fn random_out_point() -> packed::OutPoint {
    let index: u32 = thread_rng().gen_range(1..100);
    packed::OutPoint::new_builder()
        .tx_hash(random_hash())
        .index(index.pack())
        .build()
}

fn random_cell(
    block_number: BlockNumber,
    output: packed::CellOutput,
    output_data: Vec<u8>,
) -> Cell {
    let tx_index: u32 = thread_rng().gen_range(1..100);
    Cell {
        output: output.into(),
        output_data: Some(output_data.pack().into()),
        out_point: random_out_point().into(),
        block_number: block_number.into(),
        tx_index: tx_index.into(),
    }
}

fn load_data_from_file(path: &str) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn test_update_eth_client() {
    let tmp_dir = TempDir::new().unwrap();
    let mut chain = {
        let ckb_config = CkbChainConfig {
            id: ChainId::new("chainA".to_string(), 10),
            ckb_rpc: Url::from_str("http://ckb_rpc").unwrap(),
            ckb_indexer_rpc: Url::from_str("http://ckb_indexer_rpc").unwrap(),
            lightclient_contract_typeargs: h256!("0x123"),
            key_name: "ckb-chain-test".to_string(),
            data_dir: tmp_dir.path().to_path_buf(),
        };
        let config = ChainConfig::Ckb(ckb_config);
        let rt = Arc::new(TokioRuntime::new().unwrap());

        CkbChain::bootstrap(config, rt).unwrap()
    };

    let rpc_client = Arc::clone(&chain.rpc_client);

    let chain_info = r#"
        {
          "alerts": [],
          "chain": "ckb-dev",
          "difficulty": "0x10000",
          "epoch": "0x100",
          "is_initial_block_download": true,
          "median_time": "0x5cd2b105"
        }"#;
    rpc_client.set_blockchain_info(Some(chain_info));

    {
        let key = {
            let mnemonic =
                "feed label choose question decrease slab regular humor salmon wheel slab inform";
            let hd_path = StandardHDPath::from_str(HD_PATH).unwrap();
            let network = chain.network().unwrap();
            let is_mainnet = network == NetworkType::Mainnet;
            let account_prefix = if is_mainnet { "ckb" } else { "ckt" };
            let address_type = AddressType::Ckb { is_mainnet };
            Secp256k1KeyPair::from_mnemonic(mnemonic, &hd_path, &address_type, account_prefix)
                .unwrap()
        };
        let key_name = chain.config.key_name.clone();
        chain.keybase_mut().add_key(&key_name, key).unwrap();
    }

    {
        let contract_type_args = chain
            .config
            .lightclient_contract_typeargs
            .as_bytes()
            .to_vec();
        let contract = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(contract_type_args.pack())
            .build();
        let output = packed::CellOutput::new_builder()
            .type_(Some(contract.clone()).pack())
            .build_exact_capacity(Capacity::bytes(100_000).unwrap())
            .unwrap();
        let cell = random_cell(1001, output, Default::default());
        let key: SearchKey = CellQueryOptions::new(contract, PrimaryScriptType::Type).into();
        rpc_client.add_cell(&key, cell);
    }

    {
        let address = chain.tx_assembler_address().unwrap();
        let lock_script: packed::Script = address.payload().into();
        let output = packed::CellOutput::new_builder()
            .lock(lock_script.clone())
            .build_exact_capacity(Capacity::bytes(100_000).unwrap())
            .unwrap();
        let cell = random_cell(1002, output, Default::default());
        let key: SearchKey = CellQueryOptions::new(lock_script, PrimaryScriptType::Lock).into();
        rpc_client.add_cell(&key, cell);
    }

    let updates_part_1 = {
        let filepath = format!("{}/{}", TESTDATA_DIR, "headers-part-1.json");
        let headers_str = read_to_string(&filepath).unwrap();
        let headers: Vec<EthHeader> = serde_json::from_str(&headers_str).unwrap();
        headers
            .into_iter()
            .map(|header| EthUpdate {
                finalized_header: header,
                ..Default::default()
            })
            .collect::<Vec<_>>()
    };

    let result = chain.update_eth_client(updates_part_1);
    assert!(result.is_ok());

    let txs_len = rpc_client.get_transactions_len();
    assert_eq!(txs_len, 1);

    let tx_create_client = rpc_client.get_transaction_by_index(0).unwrap();

    {
        let filepath = format!("{}/{}", TESTDATA_DIR, "client.data");
        let expected_data = load_data_from_file(&filepath);
        let actual_data = tx_create_client.outputs_data[0].as_bytes().to_vec();
        assert_eq!(expected_data, actual_data);
    }

    rpc_client.clear_cells();

    {
        let contract_type_args = chain
            .config
            .lightclient_contract_typeargs
            .as_bytes()
            .to_vec();
        let contract = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(contract_type_args.pack())
            .build();
        let output = packed::CellOutput::new_builder()
            .type_(Some(contract.clone()).pack())
            .build_exact_capacity(Capacity::bytes(100_000).unwrap())
            .unwrap();
        let cell = random_cell(1003, output, Default::default());
        let key: SearchKey = CellQueryOptions::new(contract, PrimaryScriptType::Type).into();
        rpc_client.add_cell(&key, cell);
    }

    {
        let address = chain.tx_assembler_address().unwrap();
        let lock_script: packed::Script = address.payload().into();
        let output = packed::CellOutput::new_builder()
            .lock(lock_script.clone())
            .build_exact_capacity(Capacity::bytes(100_000).unwrap())
            .unwrap();
        let cell = random_cell(1004, output, Default::default());
        let key: SearchKey = CellQueryOptions::new(lock_script, PrimaryScriptType::Lock).into();
        rpc_client.add_cell(&key, cell);
    }

    {
        let contract_type_args = chain
            .config
            .lightclient_contract_typeargs
            .as_bytes()
            .to_vec();
        let contract_type_script = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(contract_type_args.pack())
            .build();
        let type_hash = contract_type_script.calc_script_hash();
        let client_as_type_args = chain.id().to_string().as_bytes().to_vec();
        let contract = packed::Script::new_builder()
            .code_hash(type_hash.clone())
            .hash_type(ScriptHashType::Type.into())
            .args(client_as_type_args.pack())
            .build();
        let key: SearchKey = CellQueryOptions::new(contract, PrimaryScriptType::Type).into();
        let output: packed::CellOutput = tx_create_client.outputs[0].clone().into();
        let output_data: Vec<u8> = tx_create_client.outputs_data[0].as_bytes().to_vec();
        let cell = random_cell(1005, output, output_data);
        rpc_client.add_cell(&key, cell);
    }

    let updates_part_2 = {
        let filepath = format!("{}/{}", TESTDATA_DIR, "headers-part-2.json");
        let headers_str = read_to_string(&filepath).unwrap();
        let headers: Vec<EthHeader> = serde_json::from_str(&headers_str).unwrap();
        headers
            .into_iter()
            .map(|header| EthUpdate {
                finalized_header: header,
                ..Default::default()
            })
            .collect::<Vec<_>>()
    };

    let result = chain.update_eth_client(updates_part_2);
    assert!(result.is_ok());

    let txs_len = rpc_client.get_transactions_len();
    assert_eq!(txs_len, 2);

    {
        let tx_update_client = rpc_client.get_transaction_by_index(1).unwrap();
        let filepath = format!("{}/{}", TESTDATA_DIR, "new_client.data");
        let expected_data = load_data_from_file(&filepath);
        let actual_data = tx_update_client.outputs_data[0].as_bytes().to_vec();
        assert_eq!(expected_data, actual_data);
    }
}
