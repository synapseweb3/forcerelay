use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, ChainInfo, HeaderView, JsonBytes, OutPoint,
    OutputsValidator, RawTxPool, Transaction, TransactionAndWitnessProof,
    TransactionWithStatusResponse, TxPoolInfo,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Pagination, SearchKey};
use ckb_types::H256;
use std::{future::Future, pin::Pin};

use crate::error::Error;

pub type Response<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

pub trait CkbReader {
    fn get_blockchain_info(&self) -> Response<ChainInfo>;

    fn get_block_by_number(&self, number: BlockNumber) -> Response<BlockView>;

    fn get_block(&self, hash: &H256) -> Response<BlockView>;

    fn get_tip_header(&self) -> Response<HeaderView>;

    fn get_transaction(&self, hash: &H256) -> Response<Option<TransactionWithStatusResponse>>;

    fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> Response<CellWithStatus>;

    fn get_txs_by_hashes(
        &self,
        hashes: Vec<H256>,
    ) -> Response<Vec<Option<TransactionWithStatusResponse>>>;

    fn get_transaction_and_witness_proof(
        &self,
        tx_hashes: Vec<H256>,
        block_hash: H256,
    ) -> Response<TransactionAndWitnessProof>;

    fn fetch_live_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> Response<Pagination<Cell>>;

    // For debugging purposes.
    fn get_raw_tx_pool(&self, verbose: bool) -> Response<RawTxPool>;

    fn tx_pool_info(&self) -> Response<TxPoolInfo>;
}

pub trait CkbWriter {
    fn send_transaction(
        &self,
        tx: &Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> Response<H256>;
}
