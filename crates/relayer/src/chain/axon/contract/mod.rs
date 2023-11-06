use ckb_jsonrpc_types::{HeaderView, JsonBytes, ScriptHashType};
use emitter_core::types::{IndexerScriptSearchMode, RpcSearchKey, RpcSearchKeyFilter, ScriptType};

mod ibc_handler;
pub use ibc_handler::*;

pub mod ckb_light_client;
pub mod image_cell;

impl From<Script> for ckb_jsonrpc_types::Script {
    fn from(value: Script) -> Self {
        let hash_type = if value.hash_type == 0 {
            ScriptHashType::Data
        } else if value.hash_type == 1 {
            ScriptHashType::Type
        } else {
            ScriptHashType::Data1
        };
        Self {
            code_hash: value.code_hash.into(),
            hash_type,
            args: JsonBytes::from_vec(value.args.to_vec()),
        }
    }
}

impl From<&SearchKey> for RpcSearchKey {
    fn from(value: &SearchKey) -> Self {
        let script_type = if value.script_type == 0 {
            ScriptType::Lock
        } else {
            ScriptType::Type
        };
        let search_mode = if value.script_search_mode == 0 {
            IndexerScriptSearchMode::Prefix
        } else {
            IndexerScriptSearchMode::Exact
        };
        let filter = if let Some(filter) = value.filter.first() {
            let json_uint_array = |v: [u64; 2]| [v[0].into(), v[1].into()];
            Some(RpcSearchKeyFilter {
                script: Some(filter.script.clone().into()),
                script_len_range: Some(json_uint_array(filter.script_len_range)),
                output_data_len_range: Some(json_uint_array(filter.output_data_len_range)),
                output_capacity_range: Some(json_uint_array(filter.output_data_capacity_range)),
            })
        } else {
            None
        };
        Self {
            script: value.script.clone().into(),
            script_type,
            script_search_mode: Some(search_mode),
            filter,
        }
    }
}

impl From<ckb_jsonrpc_types::Script> for image_cell::Script {
    fn from(value: ckb_jsonrpc_types::Script) -> Self {
        let hash_type = match value.hash_type {
            ScriptHashType::Data => 0,
            ScriptHashType::Type => 1,
            ScriptHashType::Data1 => 2,
        };

        image_cell::Script {
            args: value.args.to_owned().into_bytes().into(),
            code_hash: value.code_hash.to_owned().into(),
            hash_type,
        }
    }
}

impl From<ckb_jsonrpc_types::OutPoint> for image_cell::OutPoint {
    fn from(value: ckb_jsonrpc_types::OutPoint) -> Self {
        image_cell::OutPoint {
            tx_hash: value.tx_hash.to_owned().into(),
            index: value.index.into(),
        }
    }
}

impl From<HeaderView> for ckb_light_client::Header {
    fn from(value: HeaderView) -> Self {
        ckb_light_client::Header {
            version: value.inner.version.into(),
            compact_target: value.inner.compact_target.into(),
            timestamp: value.inner.timestamp.into(),
            number: value.inner.number.into(),
            epoch: value.inner.epoch.into(),
            parent_hash: value.inner.parent_hash.to_owned().into(),
            transactions_root: value.inner.transactions_root.to_owned().into(),
            proposals_hash: value.inner.proposals_hash.to_owned().into(),
            uncles_hash: value.inner.extra_hash.to_owned().into(),
            dao: value.inner.dao.0,
            nonce: value.inner.nonce.into(),
            block_hash: value.hash.to_owned().into(),
        }
    }
}

#[cfg(test)]
mod contract_generator {
    use ethers::contract::Abigen;

    fn generate(contract_name: &str, abi_source: &str, output_path: &str) {
        Abigen::new(contract_name, abi_source)
            .unwrap()
            .generate()
            .unwrap()
            .write_to_file(output_path)
            .unwrap();
    }

    #[ignore = "mannual run"]
    #[test]
    fn generate_ibc_handler() {
        generate(
            "OwnableIBCHandler",
            "./src/chain/axon/contract/abi/OwnableIBCHandler.json",
            "./src/chain/axon/contract/ibc_handler.rs",
        )
    }

    #[ignore = "mannual run"]
    #[test]
    fn generate_image_cell() {
        generate(
            "image_cell_contract",
            "./src/chain/axon/contract/abi/ImageCellContract.json",
            "./src/chain/axon/contract/image_cell.rs",
        )
    }

    #[ignore = "mannual run"]
    #[test]
    fn generate_light_client() {
        generate(
            "ckb_light_client_contract",
            "./src/chain/axon/contract/abi/LightClientContract.json",
            "./src/chain/axon/contract/ckb_light_client.rs",
        )
    }
}
