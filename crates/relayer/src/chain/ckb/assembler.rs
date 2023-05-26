#![allow(dead_code)]

use async_trait::async_trait;
use ckb_sdk::{
    constants::TYPE_ID_CODE_HASH,
    traits::{LiveCell, PrimaryScriptType},
    Address,
};
use ckb_types::{
    core::{Capacity, DepType, ScriptHashType, TransactionView},
    packed,
    prelude::*,
    H256,
};
use eth_light_client_in_ckb_verification::types::packed::{
    Client as PackedClient, ClientReader as PackedClientReader, ProofUpdate as PackedProofUpdate,
    ClientInfo as PackedClientInfo, ClientInfoReader as PackedClientInfoReader,
    ClientTypeArgs as PackedClientTypeArgs,
};

use super::{
    prelude::{CellSearcher, TxCompleter},
    rpc_client::RpcClient, utils,
};
use crate::error::Error;

fn make_typeid_script(type_args: Vec<u8>) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.0.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(type_args.pack())
        .build()
}

fn make_lightclient_script(script_typehash: packed::Byte32, args: Vec<u8>) -> packed::Script {
    packed::Script::new_builder()
        .code_hash(script_typehash)
        .hash_type(ScriptHashType::Type.into())
        .args(args.pack())
        .build()
}

async fn search_contract_cell<S: CellSearcher + Sync + ?Sized>(
    searcher: &S,
    script: &packed::Script,
    typeid_args: &H256,
) -> Result<LiveCell, Error> {
    let contract = searcher
        .search_cell(script, PrimaryScriptType::Type)
        .await?;
    let cell = match contract {
        Some(cell) => cell,
        None => {
            return Err(Error::rpc_response(format!(
                "contract not found: {}",
                hex::encode(typeid_args)
            )));
        }
    };
    Ok(cell)
}

// fn create_multi_client_cells(client_num: u8) {
//     let info_cell = todo!();
//     let client_cells = todo!();

// }

pub struct UpdateCells {
    oldest: LiveCell,
    latest: LiveCell,
    info: LiveCell,
}

#[async_trait]
pub trait TxAssembler: CellSearcher + TxCompleter {
    async fn fetch_update_cells(
        &self,
        contract_typeid_args: &H256,
        client_type_args: &PackedClientTypeArgs,
    ) -> Result<Option<UpdateCells>, Error> {
        let contract_typescript = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
        let type_hash = contract_typescript.calc_script_hash();
        // There are at most 255 cells
        let cells_count = u8::from(client_type_args.cells_count().as_reader());
        let cells = self
            .search_cells_by_typescript(&type_hash, client_type_args.as_slice(), cells_count as u32)
            .await?;

        // As for the error handling here, the only "allowable" error is that user supply a wrong client type args,
        // and we can't find any cells for it on chain. Otherwise, it means the on-chain data is corrupted.
        if cells.len() == 0 {
            return Ok(None)
        } else if cells.len() != cells_count as usize {
            panic!(
                "fetched client cells count not match: expect {}, actual {}",
                cells_count,
                cells.len()
            );
        }
        
        let mut client_cells = vec![];
        let mut client_info_cell_opt = None;
        for cell in cells {
            if PackedClientReader::verify(&cell.output_data, false).is_ok() {
                client_cells.push(cell);
            } else if PackedClientInfoReader::verify(&cell.output_data, false).is_ok() {
                let prev = client_info_cell_opt.replace(cell);
                if prev.is_some() {
                    panic!(
                        "multi client cell has more than one client info:\nfirst:\n{:?}\nsecond:\n{:?}",
                        PackedClientInfo::new_unchecked(prev.unwrap().output_data),
                        PackedClientInfo::new_unchecked(cell.output_data),
                    );
                }
            } else {
                panic!("multi client cell has invalid data: {:?}", cell.output_data);
            }
        }

        let Some(client_info_cell) = client_info_cell_opt else {
            panic!("on-chain data corrupted: client info cell not found");
        };
        let client_info = PackedClientInfo::new_unchecked(client_info_cell.output_data.clone());
        let latest_id = u8::from(client_info.last_id().as_reader());
        // -1 is for the client info cell
        let oldest_id = (latest_id + 1) % (cells_count - 1);

        let oldest = None;
        let latest = None;

        for cell in client_cells {
            let client = PackedClient::new_unchecked(cell.output_data.clone());
            let client_id = u8::from(client.id().as_reader());
            if client_id == latest_id {
                latest.replace(cell).expect("on-chain data corrupted");
            } else if client_id == oldest_id {
                oldest.replace(cell).expect("on-chain data corrupted");
            }
        }
        let (Some(oldest), Some(latest)) = (oldest, latest) else {
            panic!("on-chain data corrupted: oldest or latest client not found");
        };
        let update_cells = UpdateCells {
            oldest,
            latest,
            info: client_info_cell,
        };

        Ok(Some(update_cells))
    }

    async fn fetch_packed_client(
        &self,
        contract_typeid_args: &H256,
        client_id: &String,
    ) -> Result<Option<PackedClient>, Error> {
        let contract_typescript = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
        let type_hash = contract_typescript.calc_script_hash();
        let lightclient_cell_opt = self
            .search_cell_by_typescript(&type_hash, &client_id.as_bytes().to_vec())
            .await?;
        match lightclient_cell_opt {
            Some(cell) => {
                if let Err(err) = PackedClientReader::verify(&cell.output_data, false) {
                    Err(Error::rpc_response(format!("client format error: {}", err)))
                } else {
                    Ok(Some(PackedClient::new_unchecked(cell.output_data)))
                }
            }
            None => Ok(None),
        }
    }

    async fn assemble_updates_into_transaction(
        &self,
        address: &Address,
        packed_client: PackedClient,
        packed_proof_update: PackedProofUpdate,
        lock_typeid_args: &H256,
        contract_typeid_args: &H256,
        client_id: &String,
    ) -> Result<(TransactionView, Vec<packed::CellOutput>), Error> {
        // find celldeps by searching live cells according typeid_args
        let contract_typescript = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
        let contract_cell_dep = {
            let contract_cell =
                search_contract_cell(self, &contract_typescript, contract_typeid_args).await?;
            packed::CellDep::new_builder()
                .out_point(contract_cell.out_point)
                .dep_type(DepType::Code.into())
                .build()
        };
        let mock_lockscript = make_typeid_script(lock_typeid_args.as_bytes().to_vec());
        let mock_lock_celldep = {
            let mock_cell = search_contract_cell(self, &mock_lockscript, lock_typeid_args).await?;
            packed::CellDep::new_builder()
                .out_point(mock_cell.out_point)
                .dep_type(DepType::Code.into())
                .build()
        };
        // search light-client cell by lightclient contract type_id hash
        let contract_typehash = contract_typescript.calc_script_hash();
        let lightclient_cell_opt = self
            .search_cell_by_typescript(&contract_typehash, &client_id.as_bytes().to_vec())
            .await?;
        // build Lightclient Lockscript and Typescript
        let pubkey_hash = address.payload().args();
        let lightclient_lock =
            make_lightclient_script(mock_lockscript.calc_script_hash(), pubkey_hash.to_vec());
        let lightclient_type =
            make_lightclient_script(contract_typehash, client_id.clone().into_bytes());
        // assemble Lightclient output cell
        let output_data = packed_client.as_slice().pack();
        let output_cell = packed::CellOutput::new_builder()
            .lock(lightclient_lock)
            .type_(Some(lightclient_type).pack())
            .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
            .expect("build ibc contract output");
        let mut inputs_cell_as_output = vec![];
        let mut inputs_cell = vec![];
        let mut inputs_capacity: u64 = 0;
        if let Some(lightclient_cell) = lightclient_cell_opt {
            inputs_cell.push(packed::CellInput::new(lightclient_cell.out_point, 0));
            inputs_capacity += Unpack::<u64>::unpack(&lightclient_cell.output.capacity());
            inputs_cell_as_output.push(lightclient_cell.output);
        }
        // assemble Lightclient witness
        let witness = {
            let input_type_args = packed::BytesOpt::new_builder()
                .set(Some(packed_proof_update.as_slice().pack()))
                .build();
            let witness_args = packed::WitnessArgs::new_builder()
                .input_type(input_type_args)
                .build();
            witness_args.as_bytes()
        };
        // assemble transaction
        let tx = TransactionView::new_advanced_builder()
            .inputs(inputs_cell)
            .output(output_cell)
            .output_data(output_data)
            .witness(witness.pack())
            .cell_dep(contract_cell_dep)
            .cell_dep(mock_lock_celldep)
            .build();
        let fee_rate = 3000;
        let (tx, mut new_inputs) = self
            .complete_tx_with_secp256k1_change(tx, address, inputs_capacity, fee_rate)
            .await?;
        // collect input cells to support signing process (calculating input group)
        inputs_cell_as_output.append(&mut new_inputs);
        Ok((tx, inputs_cell_as_output))
    }
}

impl TxAssembler for RpcClient {}
