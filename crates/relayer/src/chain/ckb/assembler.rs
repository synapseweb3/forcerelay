#![allow(dead_code)]

use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_indexer::{Cell as IndexerCell, SearchKey};
use ckb_sdk::traits::{CellQueryOptions, PrimaryScriptType};
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::core::{ScriptHashType, TransactionView};
use ckb_types::packed::{Byte32, CellOutput, Script};
use ckb_types::{prelude::*, H256};
use ibc_relayer_types::clients::ics07_eth::types::Update;
use secp256k1::PublicKey;
use std::sync::Arc;

use super::rpc_client::RpcClient;
use crate::error::Error;

pub struct TxAssembler {
    ckb_rpc: Arc<RpcClient>,
    address: Address,
}

impl TxAssembler {
    pub fn new(ckb_rpc: Arc<RpcClient>, pubkey: &PublicKey, network: NetworkType) -> Self {
        let address_payload = AddressPayload::from_pubkey(pubkey);
        let address = Address::new(network, address_payload, true);
        Self { ckb_rpc, address }
    }

    pub async fn assemble_updates_into_transaction(
        &self,
        _updates: &[Update],
        contract_typeid_args: &H256,
        client_id: &String,
    ) -> Result<(TransactionView, Vec<CellOutput>), Error> {
        let contract_typescript = Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(contract_typeid_args.as_bytes().to_vec().pack())
            .build();
        let _contract_cell = {
            let contract = search_cell(&self.ckb_rpc, &contract_typescript).await?;
            match contract {
                Some(cell) => cell,
                None => {
                    return Err(Error::rpc_response(format!(
                        "contract not found: {}",
                        hex::encode(&contract_typeid_args)
                    )));
                }
            }
        };
        let _lightclient_cell = search_cell_by_typescript(
            &self.ckb_rpc,
            &contract_typescript.calc_script_hash(),
            &client_id.as_bytes().to_vec(),
        )
        .await?;
        todo!()
    }
}

async fn search_cell(rpc: &RpcClient, typescript: &Script) -> Result<Option<IndexerCell>, Error> {
    let search: SearchKey =
        CellQueryOptions::new(typescript.clone(), PrimaryScriptType::Type).into();
    let result = rpc
        .fetch_live_cells(search, 1, None)
        .await
        .map_err(|e| Error::rpc_response(e.to_string()))?;
    Ok(result.objects.first().cloned())
}

async fn search_cell_by_typescript(
    rpc: &RpcClient,
    code_hash: &Byte32,
    type_args: &Vec<u8>,
) -> Result<Option<IndexerCell>, Error> {
    let typescript = Script::new_builder()
        .code_hash(code_hash.clone())
        .hash_type(ScriptHashType::Type.into())
        .args(type_args.pack())
        .build();
    search_cell(rpc, &typescript).await
}
