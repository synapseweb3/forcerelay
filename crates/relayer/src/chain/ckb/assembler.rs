#![allow(dead_code)]

use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_indexer::SearchKey;
use ckb_sdk::traits::{CellQueryOptions, LiveCell, PrimaryScriptType};
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::core::{DepType, ScriptHashType, TransactionView};
use ckb_types::{packed, prelude::*, H256};
use eth_light_client_in_ckb_verification::types::packed::{
    Client as PackedClient, ProofUpdate as PackedProofUpdate,
};
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
        packed_client: PackedClient,
        packed_proof_update: PackedProofUpdate,
        contract_typeid_args: &H256,
        client_id: &String,
    ) -> Result<(TransactionView, Vec<packed::CellOutput>), Error> {
        let contract_typescript = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(contract_typeid_args.as_bytes().to_vec().pack())
            .build();
        let contract_cell = {
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
        let contract_cell_dep = packed::CellDep::new_builder()
            .out_point(contract_cell.out_point)
            .dep_type(DepType::Code.into())
            .build();

        let type_hash = contract_typescript.calc_script_hash();
        let lightclient_cell_opt =
            search_cell_by_typescript(&self.ckb_rpc, &type_hash, &client_id.as_bytes().to_vec())
                .await?;

        let (input_cell, output_cell) = if let Some(lightclient_cell) = lightclient_cell_opt {
            let input_cell = packed::CellInput::new(lightclient_cell.out_point, 0);
            let output_cell = packed::CellOutput::new_builder()
                // TODO minus the fee
                .capacity(lightclient_cell.output.capacity())
                // TODO set the lock script for client cell
                //.lock(lock)
                .type_(Some(contract_typescript).pack())
                .build();
            (input_cell, output_cell)
        } else {
            // TODO If there is no client cell, an input cell to supply capacity is still be required.
            todo!()
        };

        let witness = {
            let input_type_args = packed::BytesOpt::new_builder()
                .set(Some(packed_proof_update.as_slice().pack()))
                .build();
            let witness_args = packed::WitnessArgs::new_builder()
                .input_type(input_type_args)
                .build();
            witness_args.as_bytes()
        };
        let tx = TransactionView::new_advanced_builder()
            // TODO a cell dep for the lock script
            .cell_dep(contract_cell_dep)
            .input(input_cell)
            .output(output_cell)
            .output_data(packed_client.as_slice().pack())
            .witness(witness.pack())
            .build();

        Ok((tx, vec![]))
    }
}

async fn search_cell(
    rpc: &RpcClient,
    typescript: &packed::Script,
) -> Result<Option<LiveCell>, Error> {
    let search: SearchKey =
        CellQueryOptions::new(typescript.clone(), PrimaryScriptType::Type).into();
    let result = rpc
        .fetch_live_cells(search, 1, None)
        .await
        .map_err(|e| Error::rpc_response(e.to_string()))?;
    Ok(result.objects.first().cloned().map(Into::into))
}

async fn search_cell_by_typescript(
    rpc: &RpcClient,
    code_hash: &packed::Byte32,
    type_args: &Vec<u8>,
) -> Result<Option<LiveCell>, Error> {
    let typescript = packed::Script::new_builder()
        .code_hash(code_hash.clone())
        .hash_type(ScriptHashType::Type.into())
        .args(type_args.pack())
        .build();
    search_cell(rpc, &typescript).await
}
