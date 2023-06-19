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
    Client as PackedClient, ClientInfo as PackedClientInfo,
    ClientInfoReader as PackedClientInfoReader, ClientReader as PackedClientReader,
    ClientTypeArgs as PackedClientTypeArgs, Hash as PackedHash, ProofUpdate as PackedProofUpdate,
};

use super::{
    prelude::{CellSearcher, TxCompleter},
    rpc_client::RpcClient,
    utils,
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

pub struct UpdateCells {
    pub oldest: LiveCell,
    pub latest: LiveCell,
    pub info: LiveCell,
}

#[async_trait]
pub trait TxAssembler: CellSearcher + TxCompleter {
    async fn fetch_multi_client_cells(
        &self,
        contract_typeid_args: &H256,
        client_type_args: &PackedClientTypeArgs,
    ) -> Result<Option<(Vec<LiveCell>, LiveCell)>, Error> {
        let contract_typescript = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
        let type_hash = contract_typescript.calc_script_hash();
        // There are at most 255 cells
        let cells_count = u8::from(client_type_args.cells_count().as_reader());
        let cells = self
            .search_cells_by_typescript(&type_hash, client_type_args.as_slice(), cells_count as u32)
            .await?;

        // As for the error handling here, the only "allowable" error is that user supply a wrong client type args,
        // and we can't find any cells for it on chain. Otherwise, it means the on-chain data is corrupted.
        if cells.is_empty() {
            return Ok(None);
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
                let prev = client_info_cell_opt.replace(cell.clone());
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
        Ok(Some((client_cells, client_info_cell)))
    }

    async fn fetch_clients_and_info(
        &self,
        contract_typeid_args: &H256,
        client_type_args: &PackedClientTypeArgs,
    ) -> Result<Option<(Vec<PackedClient>, PackedClientInfo)>, Error> {
        let (client_cells, client_info_cell) = match self
            .fetch_multi_client_cells(contract_typeid_args, client_type_args)
            .await?
        {
            Some(cells) => cells,
            None => return Ok(None),
        };

        let mut clients = vec![];
        for cell in client_cells {
            let client = PackedClient::new_unchecked(cell.output_data.clone());
            clients.push(client);
        }

        let client_info = PackedClientInfo::new_unchecked(client_info_cell.output_data);
        Ok(Some((clients, client_info)))
    }

    async fn fetch_update_cells(
        &self,
        contract_typeid_args: &H256,
        client_type_args: &PackedClientTypeArgs,
    ) -> Result<Option<UpdateCells>, Error> {
        let (client_cells, client_info_cell) = match self
            .fetch_multi_client_cells(contract_typeid_args, client_type_args)
            .await?
        {
            Some(cells) => cells,
            None => return Ok(None),
        };

        let cells_count = u8::from(client_type_args.cells_count().as_reader());
        let client_info = PackedClientInfo::new_unchecked(client_info_cell.output_data.clone());
        let latest_id = u8::from(client_info.last_id().as_reader());

        let oldest_id = if latest_id + 2 < cells_count {
            latest_id + 1
        } else {
            0
        };

        let mut oldest = None;
        let mut latest = None;

        for cell in client_cells {
            let client = PackedClient::new_unchecked(cell.output_data.clone());
            let client_id = u8::from(client.id().as_reader());
            if client_id == latest_id {
                latest.replace(cell.clone());
            }
            if client_id == oldest_id {
                oldest.replace(cell);
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

    async fn build_lock_script(
        &self,
        lock_typeid_args: &H256,
    ) -> Result<(packed::Script, packed::CellDep), Error> {
        let lock_contract = make_typeid_script(lock_typeid_args.as_bytes().to_vec());
        let lock_contract_hash = lock_contract.calc_script_hash();
        let lock_contract_celldep = {
            let cell = search_contract_cell(self, &lock_contract, lock_typeid_args).await?;
            packed::CellDep::new_builder()
                .out_point(cell.out_point)
                .dep_type(DepType::Code.into())
                .build()
        };
        let lock_script = packed::Script::new_builder()
            .code_hash(lock_contract_hash)
            .hash_type(ScriptHashType::Type.into())
            // TODO: currently using empty lock
            // .args(...)
            .build();
        Ok((lock_script, lock_contract_celldep))
    }

    async fn assemble_create_multi_client_transaction(
        &self,
        address: &Address,
        clients: Vec<PackedClient>,
        client_info: PackedClientInfo,
        lock_typeid_args: &H256,
        contract_typeid_args: &H256,
        packed_proof_update: PackedProofUpdate,
    ) -> Result<(TransactionView, Vec<packed::CellOutput>, H256), Error> {
        // Build lock script
        let (lock_script, lock_contract_celldep) = self.build_lock_script(lock_typeid_args).await?;

        // Build type script
        let lc_contract = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
        let lc_contract_hash = lc_contract.calc_script_hash();
        let lc_contract_celldep = {
            let cell = search_contract_cell(self, &lc_contract, contract_typeid_args).await?;
            packed::CellDep::new_builder()
                .out_point(cell.out_point)
                .dep_type(DepType::Code.into())
                .build()
        };
        // We have to get one input cell to calculate the type id for those new cells.
        let mut _excessive_capacity = 0;
        let input_cells = self
            .search_cells_by_address_and_capacity(address, 1, &mut _excessive_capacity)
            .await?;
        let inputs_capacity: u64 = input_cells
            .iter()
            .map(|c| Unpack::<u64>::unpack(&c.output.capacity()))
            .sum();
        let (inputs, mut inputs_as_cell_outputs): (
            Vec<packed::CellInput>,
            Vec<packed::CellOutput>,
        ) = input_cells
            .into_iter()
            .map(|cell| {
                let input = packed::CellInput::new(cell.out_point, 0);
                let input_as_cell_output = cell.output;
                (input, input_as_cell_output)
            })
            .unzip();

        let cells_count = (clients.len() + 1) as u8;
        let new_cells_type_id = {
            let first = inputs.first().expect("input cell not found");
            let type_id = utils::calculate_type_id(first, cells_count as usize);
            H256(type_id)
        };
        let type_script: packed::Script = {
            let packed_type_id = PackedHash::new_builder()
                .set(new_cells_type_id.0.map(packed::Byte::new))
                .build();
            let client_type_args = PackedClientTypeArgs::new_builder()
                .cells_count(packed::Byte::new(cells_count))
                .type_id(packed_type_id)
                .build();
            packed::Script::new_builder()
                .code_hash(lc_contract_hash)
                .hash_type(ScriptHashType::Type.into())
                .args(client_type_args.as_slice().pack())
                .build()
        };

        let mut outputs_data = clients
            .into_iter()
            .map(|client| client.as_slice().pack())
            .collect::<Vec<_>>();
        outputs_data.push(client_info.as_slice().pack());
        let outputs = outputs_data
            .iter()
            .map(|data| {
                packed::CellOutput::new_builder()
                    .lock(lock_script.clone())
                    .type_(Some(type_script.clone()).pack())
                    .build_exact_capacity(Capacity::bytes(data.len()).unwrap())
                    .expect("build ibc contract output")
            })
            .collect::<Vec<_>>();

        let witness = {
            let input_type_args = packed::BytesOpt::new_builder()
                .set(Some(packed_proof_update.as_slice().pack()))
                .build();
            let witness_args = packed::WitnessArgs::new_builder()
                .input_type(input_type_args)
                .build();
            witness_args.as_bytes().pack()
        };
        let tx = TransactionView::new_advanced_builder()
            .inputs(inputs)
            .outputs(outputs)
            .outputs_data(outputs_data)
            .witness(witness)
            .cell_dep(lc_contract_celldep)
            .cell_dep(lock_contract_celldep)
            .build();

        let fee_rate = 3000;
        let (tx, mut new_inputs_as_cell_outputs) = self
            .complete_tx_with_secp256k1_change(tx, address, inputs_capacity, fee_rate)
            .await?;
        inputs_as_cell_outputs.append(&mut new_inputs_as_cell_outputs);
        Ok((tx, inputs_as_cell_outputs, new_cells_type_id))
    }

    async fn assemble_update_multi_client_transaction(
        &self,
        address: &Address,
        update_cells: UpdateCells,
        updated_client: PackedClient,
        client_type_args: &PackedClientTypeArgs,
        lock_typeid_args: &H256,
        contract_typeid_args: &H256,
        packed_proof_update: PackedProofUpdate,
    ) -> Result<(TransactionView, Vec<packed::CellOutput>), Error> {
        let UpdateCells {
            oldest: oldest_cell,
            latest: latest_cell,
            info: info_cell,
        } = update_cells;
        let latest_client_cell_dep = packed::CellDep::new_builder()
            .out_point(latest_cell.out_point)
            .dep_type(DepType::Code.into())
            .build();

        // Build lock script
        let (lock_script, lock_contract_celldep) = self.build_lock_script(lock_typeid_args).await?;

        // Build type script
        let (type_script, lc_contract_celldep) = {
            let lc_contract = make_typeid_script(contract_typeid_args.as_bytes().to_vec());
            let lc_contract_hash = lc_contract.calc_script_hash();
            let lc_contract_celldep = {
                let cell = search_contract_cell(self, &lc_contract, contract_typeid_args).await?;
                packed::CellDep::new_builder()
                    .out_point(cell.out_point)
                    .dep_type(DepType::Code.into())
                    .build()
            };
            let type_script: packed::Script = packed::Script::new_builder()
                .code_hash(lc_contract_hash)
                .hash_type(ScriptHashType::Type.into())
                .args(client_type_args.as_slice().pack())
                .build();
            (type_script, lc_contract_celldep)
        };

        let (new_info_output, new_info_output_data) = {
            let last_id = {
                let oldest_client = PackedClient::new_unchecked(oldest_cell.output_data.clone());
                u8::from(oldest_client.id().as_reader())
            };

            let info = PackedClientInfo::new_unchecked(info_cell.output_data.clone())
                .as_builder()
                .last_id(last_id.into())
                .build();
            let output_data = info.as_slice().pack();
            let output = packed::CellOutput::new_builder()
                .lock(lock_script.clone())
                .type_(Some(type_script.clone()).pack())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .expect("build ibc contract output");
            (output, output_data)
        };

        let (new_client_output, new_client_output_data) = {
            let output_data = updated_client.as_slice().pack();
            let output = packed::CellOutput::new_builder()
                .lock(lock_script.clone())
                .type_(Some(type_script.clone()).pack())
                .build_exact_capacity(Capacity::bytes(output_data.len()).unwrap())
                .expect("build ibc contract output");
            (output, output_data)
        };

        // Later handling requires the CellOutput form of inputs.
        let input_cells = [info_cell, oldest_cell];
        let inputs_capacity: u64 = input_cells
            .iter()
            .map(|c| Unpack::<u64>::unpack(&c.output.capacity()))
            .sum();
        let (inputs, mut inputs_as_cell_outputs): (
            Vec<packed::CellInput>,
            Vec<packed::CellOutput>,
        ) = input_cells
            .into_iter()
            .map(|cell| {
                let input = packed::CellInput::new(cell.out_point, 0);
                let input_as_cell_output = cell.output;
                (input, input_as_cell_output)
            })
            .unzip();

        let witness = {
            let input_type_args = packed::BytesOpt::new_builder()
                .set(Some(packed_proof_update.as_slice().pack()))
                .build();
            let witness_args = packed::WitnessArgs::new_builder()
                .input_type(input_type_args)
                .build();
            witness_args.as_bytes().pack()
        };
        let tx = TransactionView::new_advanced_builder()
            .inputs(inputs)
            .outputs([new_info_output, new_client_output])
            .outputs_data([new_info_output_data, new_client_output_data])
            // place holder
            .witness(Default::default())
            .witness(witness)
            .cell_dep(latest_client_cell_dep)
            .cell_dep(lc_contract_celldep)
            .cell_dep(lock_contract_celldep)
            .build();

        let fee_rate = 3000;
        let (tx, mut new_inputs_as_cell_outputs) = self
            .complete_tx_with_secp256k1_change(tx, address, inputs_capacity, fee_rate)
            .await?;
        inputs_as_cell_outputs.append(&mut new_inputs_as_cell_outputs);
        Ok((tx, inputs_as_cell_outputs))
    }
}

impl TxAssembler for RpcClient {}
