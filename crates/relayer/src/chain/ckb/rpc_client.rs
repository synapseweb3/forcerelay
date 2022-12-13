#![allow(dead_code)]

use ckb_jsonrpc_types::{
    BlockNumber, BlockView, CellWithStatus, HeaderView, JsonBytes, OutPoint, OutputsValidator,
    Transaction, TransactionWithStatusResponse, Uint32,
};
use ckb_sdk::rpc::ckb_indexer::{Cell, Order, Pagination, SearchKey};
use ckb_types::H256;
use futures::FutureExt;
use reqwest::Client;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tendermint_rpc::{Error as TmError, Url};

use crate::error::Error;

#[allow(clippy::upper_case_acronyms)]
enum Target {
    CKB,
    Indexer,
}

type RPC<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

macro_rules! jsonrpc {
    ($method:expr, $id:expr, $self:ident, $return:ty$(, $params:ident$(,)?)*) => {{
        let data = format!(
            r#"{{"id": {}, "jsonrpc": "2.0", "method": "{}", "params": {}}}"#,
            $self.id.load(Ordering::Relaxed),
            $method,
            serde_json::to_value(($($params,)*)).unwrap()
        );
        $self.id.fetch_add(1, Ordering::Relaxed);

        let req_json: serde_json::Value = serde_json::from_str(&data).unwrap();

        let url = match $id {
            Target::CKB => $self.ckb_uri.clone(),
            Target::Indexer => $self.indexer_uri.clone(),
        };
        let reqwest_url = reqwest::Url::parse(&url.to_string()).unwrap();
        let c = $self.raw.post(reqwest_url).json(&req_json);
        async {
            let resp = c
                .send()
                .await
                .map_err(|_| Error::rpc(url.clone(), TmError::invalid_url(url)))?;
            let output = resp
                .json::<jsonrpc_core::response::Output>()
                .await
                .map_err(|e| Error::rpc_response(e.to_string()))?;

            match output {
                jsonrpc_core::response::Output::Success(success) => {
                    Ok(serde_json::from_value::<$return>(success.result).unwrap())
                }
                jsonrpc_core::response::Output::Failure(e) => {
                    Err(Error::rpc_response(format!("{:?}", e)))
                }
            }
        }
    }}
}

#[derive(Clone)]
pub struct RpcClient {
    raw: Client,
    ckb_uri: Url,
    indexer_uri: Url,
    id: Arc<AtomicU64>,
}

impl RpcClient {
    pub fn new(ckb_uri: &Url, indexer_uri: &Url) -> Self {
        RpcClient {
            raw: Client::new(),
            ckb_uri: ckb_uri.clone(),
            indexer_uri: indexer_uri.clone(),
            id: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn get_block_by_number(&self, number: BlockNumber) -> RPC<BlockView> {
        jsonrpc!("get_block_by_number", Target::CKB, self, BlockView, number).boxed()
    }

    pub fn get_block(&self, hash: &H256) -> RPC<BlockView> {
        jsonrpc!("get_block", Target::CKB, self, BlockView, hash).boxed()
    }

    pub fn get_tip_header(&self) -> RPC<HeaderView> {
        jsonrpc!("get_tip_header", Target::CKB, self, HeaderView).boxed()
    }

    pub fn get_transaction(&self, hash: &H256) -> RPC<Option<TransactionWithStatusResponse>> {
        jsonrpc!(
            "get_transaction",
            Target::CKB,
            self,
            Option<TransactionWithStatusResponse>,
            hash
        )
        .boxed()
    }

    pub fn get_live_cell(&self, out_point: &OutPoint, with_data: bool) -> RPC<CellWithStatus> {
        jsonrpc!(
            "get_live_cell",
            Target::CKB,
            self,
            CellWithStatus,
            out_point,
            with_data
        )
        .boxed()
    }

    pub fn send_transaction(
        &self,
        tx: &Transaction,
        outputs_validator: Option<OutputsValidator>,
    ) -> RPC<H256> {
        jsonrpc!(
            "send_transaction",
            Target::CKB,
            self,
            H256,
            tx,
            outputs_validator
        )
        .boxed()
    }

    pub fn get_txs_by_hashes(
        &self,
        hashes: Vec<H256>,
    ) -> RPC<Vec<Option<TransactionWithStatusResponse>>> {
        let mut list = Vec::with_capacity(hashes.len());
        let mut res = Vec::with_capacity(hashes.len());
        for hash in hashes {
            let task = self.get_transaction(&hash);
            list.push(tokio::spawn(task));
        }
        async {
            for i in list {
                let r = i.await.unwrap()?;
                res.push(r);
            }

            Ok(res)
        }
        .boxed()
    }

    pub fn fetch_live_cells(
        &self,
        search_key: SearchKey,
        limit: u32,
        cursor: Option<JsonBytes>,
    ) -> RPC<Pagination<Cell>> {
        let order = Order::Asc;
        let limit = Uint32::from(limit);

        jsonrpc!(
            "get_cells",
            Target::Indexer,
            self,
            Pagination<Cell>,
            search_key,
            order,
            limit,
            cursor,
        )
        .boxed()
    }
}
