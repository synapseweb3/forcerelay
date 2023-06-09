use async_trait::async_trait;
use ckb_sdk::{
    rpc::ckb_indexer::SearchKey,
    traits::{CellQueryOptions, LiveCell, PrimaryScriptType},
    Address,
};
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, ScriptHashType, TransactionView},
    packed,
    prelude::*,
};

use super::{prelude::CkbReader, rpc_client::RpcClient, sighash::get_secp256k1_celldep};
use crate::error::Error;

#[async_trait]
pub trait CellSearcher: CkbReader {
    async fn search_cell(
        &self,
        script: &packed::Script,
        script_type: PrimaryScriptType,
    ) -> Result<Option<LiveCell>, Error> {
        let search: SearchKey = CellQueryOptions::new(script.clone(), script_type).into();
        let result = self
            .fetch_live_cells(search, 1, None)
            .await
            .map_err(|e| Error::rpc_response(e.to_string()))?;
        Ok(result.objects.first().cloned().map(Into::into))
    }

    async fn search_cell_by_typescript(
        &self,
        code_hash: &packed::Byte32,
        type_args: &Vec<u8>,
    ) -> Result<Option<LiveCell>, Error> {
        let typescript = packed::Script::new_builder()
            .code_hash(code_hash.clone())
            .hash_type(ScriptHashType::Type.into())
            .args(type_args.pack())
            .build();
        self.search_cell(&typescript, PrimaryScriptType::Type).await
    }

    async fn search_cells_by_address_and_capacity(
        &self,
        address: &Address,
        need_capacity: u64,
        excessive_capacity: &mut u64,
    ) -> Result<Vec<LiveCell>, Error> {
        let lockscript: packed::Script = address.payload().into();
        let mut searched_capacity = 0;
        let mut next = None;
        let mut searched_cells = vec![];
        while searched_capacity < need_capacity {
            let search: SearchKey =
                CellQueryOptions::new(lockscript.clone(), PrimaryScriptType::Lock).into();
            let result = self
                .fetch_live_cells(search, 5, next)
                .await
                .map_err(|e| Error::rpc_response(e.to_string()))?;

            if result.objects.is_empty() {
                let errmsg = format!(
                    "no enough ckb ({searched_capacity}/{need_capacity}) on address: {address}"
                );
                return Err(Error::send_tx(errmsg));
            }

            let mut live_cells = result
                .objects
                .into_iter()
                .filter_map(|cell| {
                    if searched_capacity < need_capacity {
                        searched_capacity += Into::<u64>::into(cell.output.capacity);
                        Some(cell.into())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            searched_cells.append(&mut live_cells);
            next = Some(result.last_cursor);
        }
        *excessive_capacity = searched_capacity - need_capacity;
        Ok(searched_cells)
    }
}

#[async_trait]
pub trait TxCompleter: CellSearcher {
    async fn complete_tx_with_secp256k1_change(
        &self,
        mut tx: TransactionView,
        address: &Address,
        inputs_capacity: u64,
        fee_rate: u64,
    ) -> Result<(TransactionView, Vec<packed::CellOutput>), Error> {
        let lock_script: packed::Script = address.payload().into();
        let mut change_cell = packed::CellOutput::new_builder()
            .lock(lock_script.clone())
            .build_exact_capacity(Capacity::zero())
            .unwrap();
        let outputs_capacity = {
            let capacity = tx
                .outputs_capacity()
                .map_err(|err| Error::send_tx(err.to_string()))?
                .as_u64();
            let fee = tx.data().as_bytes().len() as u64 * fee_rate;
            capacity + fee + Unpack::<u64>::unpack(&change_cell.capacity())
        };
        let mut excessive_capacity = 0;
        let mut inputs_cell_as_output = vec![];
        if outputs_capacity > inputs_capacity {
            let need_capacity = outputs_capacity - inputs_capacity;
            let live_cells = self
                .search_cells_by_address_and_capacity(
                    address,
                    need_capacity,
                    &mut excessive_capacity,
                )
                .await?;
            let inputs_cell = live_cells
                .into_iter()
                .map(|cell| {
                    inputs_cell_as_output.push(cell.output);
                    packed::CellInput::new_builder()
                        .previous_output(cell.out_point)
                        .build()
                })
                .collect::<Vec<_>>();
            tx = tx.as_advanced_builder().inputs(inputs_cell).build();
        } else {
            excessive_capacity = inputs_capacity - outputs_capacity;
        };
        change_cell = change_cell
            .as_builder()
            .build_exact_capacity(Capacity::shannons(excessive_capacity))
            .unwrap();
        tx = tx
            .as_advanced_builder()
            .output(change_cell)
            .output_data(Bytes::new().pack())
            .cell_dep(get_secp256k1_celldep(address.network()))
            .build();
        Ok((tx, inputs_cell_as_output))
    }
}

impl CellSearcher for RpcClient {}
impl TxCompleter for RpcClient {}
