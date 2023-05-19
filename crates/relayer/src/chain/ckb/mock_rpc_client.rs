#![allow(dead_code)]
#![allow(unused_variables)]

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, ChainInfo, Header, HeaderView, JsonBytes, OutPoint,
    OutputsValidator, RawTxPool, ResponseFormat, Transaction, TransactionView,
    TransactionWithStatusResponse, TxPoolInfo, TxStatus,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Pagination, SearchKey};
use ckb_types::{packed, prelude::*, H256};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tendermint_rpc::Url;

use super::prelude::{CkbReader, CkbWriter, Response as Rpc};
use crate::error::Error;

#[derive(Clone)]
pub struct RpcClient {
    data: Arc<RwLock<RpcData>>,
}

#[derive(Default)]
struct RpcData {
    chain_info: Option<String>,

    cells: HashMap<String, Vec<Cell>>,

    transactions: Vec<Transaction>,
}

impl RpcClient {
    pub fn new(_ckb_uri: &Url, _indexer_uri: &Url) -> Self {
        Self {
            data: Arc::new(RwLock::new(RpcData::default())),
        }
    }

    pub fn set_blockchain_info(&self, chain_info: Option<&str>) {
        self.data.write().unwrap().chain_info = chain_info.map(ToOwned::to_owned);
    }

    pub fn add_cell(&self, key: &SearchKey, cell: Cell) {
        let key_string = serde_json::to_string(key).unwrap();
        self.data
            .write()
            .unwrap()
            .cells
            .entry(key_string)
            .and_modify(|v| v.push(cell.clone()))
            .or_insert_with(|| vec![cell]);
    }

    pub fn clear_cells(&self) {
        self.data.write().unwrap().cells = HashMap::default();
    }

    pub fn get_transaction_by_index(&self, index: usize) -> Option<Transaction> {
        self.data.read().unwrap().transactions.get(index).cloned()
    }

    pub fn get_transactions_len(&self) -> usize {
        self.data.read().unwrap().transactions.len()
    }
}

impl CkbReader for RpcClient {
    fn get_blockchain_info(&self) -> Rpc<ChainInfo> {
        let resp = if let Some(ref chain_info) = self.data.read().unwrap().chain_info {
            Ok(serde_json::from_str(chain_info).unwrap())
        } else {
            Err(Error::rpc_response("data is not set".to_owned()))
        };
        Box::pin(async { resp })
    }

    fn get_block_by_number(&self, number: BlockNumber) -> Rpc<BlockView> {
        todo!()
    }

    fn get_block(&self, hash: &H256) -> Rpc<BlockView> {
        let resp = BlockView {
            header: HeaderView {
                inner: Header {
                    number: 1u64.into(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        Box::pin(async { Ok(resp) })
    }

    fn get_tip_header(&self) -> Rpc<HeaderView> {
        let resp = HeaderView {
            inner: Header {
                number: u64::MAX.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        Box::pin(async { Ok(resp) })
    }

    fn get_transaction(&self, hash: &H256) -> Rpc<Option<TransactionWithStatusResponse>> {
        let transaction = ResponseFormat::<TransactionView>::json(Default::default());
        let resp = TransactionWithStatusResponse {
            transaction: Some(transaction),
            tx_status: TxStatus::committed(hash.clone()),
            cycles: None,
        };
        Box::pin(async { Ok(Some(resp)) })
    }

    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Rpc<CellWithStatus> {
        todo!()
    }

    fn get_txs_by_hashes(
        &self,
        hashes: Vec<H256>,
    ) -> Rpc<Vec<Option<TransactionWithStatusResponse>>> {
        todo!()
    }

    fn fetch_live_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Rpc<Pagination<Cell>> {
        let key_string = serde_json::to_string(&search_key).unwrap();
        let index = cursor
            .map(|json_bytes| {
                let bytes = json_bytes.as_bytes();
                let mut u32_be_bytes = [0u8; 4];
                u32_be_bytes[..].copy_from_slice(&bytes[..4]);
                u32::from_be_bytes(u32_be_bytes)
            })
            .unwrap_or(0);
        let mut cells = self
            .data
            .read()
            .unwrap()
            .cells
            .get(&key_string)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let cells_count = cells.len() as u32;
        let resp = if cells_count > index {
            let mut objects = cells.split_off(index as usize);
            objects.truncate(limit as usize);
            let new_index = index + limit;
            let new_index = if cells_count > new_index {
                new_index
            } else {
                u32::MAX
            };
            Pagination {
                objects,
                last_cursor: JsonBytes::from_vec(new_index.to_be_bytes().to_vec()),
            }
        } else {
            Pagination {
                objects: Default::default(),
                last_cursor: JsonBytes::from_vec(u32::MAX.to_be_bytes().to_vec()),
            }
        };
        Box::pin(async { Ok(resp) })
    }

    fn get_raw_tx_pool(&self, verbose: bool) -> Rpc<RawTxPool> {
        todo!()
    }

    fn tx_pool_info(&self) -> Rpc<TxPoolInfo> {
        todo!()
    }
}

impl CkbWriter for RpcClient {
    fn send_transaction(
        &self,
        tx: &Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Rpc<H256> {
        let packed_tx: packed::Transaction = tx.clone().into();
        let tx_hash = packed_tx.calc_tx_hash();
        self.data.write().unwrap().transactions.push(tx.clone());
        Box::pin(async move { Ok(tx_hash.unpack()) })
    }
}
