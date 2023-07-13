use crate::error::Error;

use async_trait::async_trait;
use axon_tools::types::{AxonBlock, CkbRelatedInfo, Metadata, Proof};
use ethers::types::{BlockId, BlockNumber};
use reqwest::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tendermint_rpc::Url;

pub type Response<T> = Result<T, Error>;

#[async_trait]
pub trait AxonRpc {
    async fn get_block_by_id(&self, block_id: BlockId) -> Response<AxonBlock>;

    async fn get_proof_by_id(&self, block_id: BlockId) -> Response<Proof>;

    async fn get_metadata_by_number(&self, block_number: BlockNumber) -> Response<Metadata>;

    async fn get_current_metadata(&self) -> Response<Metadata>;

    async fn get_ckb_related_info(&self) -> Response<CkbRelatedInfo>;
}

#[derive(Clone)]
pub struct AxonRpcClient {
    client: Client,
    url: Url,
    id: Arc<AtomicU64>,
}

impl AxonRpcClient {
    pub fn new(url: &Url) -> Self {
        Self {
            client: Client::new(),
            url: url.clone(),
            id: Arc::new(AtomicU64::new(0)),
        }
    }
}

macro_rules! jsonrpc {
    ($method:expr, $self:ident, $return:ty$(, $params:ident$(,)?)*) => {{
        let data = format!(
            r#"{{"id": {}, "jsonrpc": "2.0", "method": "{}", "params": {}}}"#,
            $self.id.load(Ordering::Relaxed),
            $method,
            serde_json::to_value(($($params,)*)).unwrap()
        );
        $self.id.fetch_add(1, Ordering::Relaxed);

        let req_json: serde_json::Value = serde_json::from_str(&data).unwrap();

        let url = $self.url.clone();
        let reqwest_url = reqwest::Url::parse(&url.to_string()).unwrap();
        let c = $self.client.post(reqwest_url).json(&req_json);
        let resp = c
            .send()
            .await
            .map_err(|e| Error::rpc_response(format!("url: {}, error: {}", url, e)))?;
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
    }}
}

#[async_trait]
impl AxonRpc for AxonRpcClient {
    async fn get_block_by_id(&self, block_id: BlockId) -> Response<AxonBlock> {
        jsonrpc!("axon_getBlockById", self, AxonBlock, block_id)
    }

    async fn get_proof_by_id(&self, block_id: BlockId) -> Response<Proof> {
        jsonrpc!("axon_getProofById", self, Proof, block_id)
    }

    async fn get_metadata_by_number(&self, block_number: BlockNumber) -> Response<Metadata> {
        jsonrpc!("axon_getMetadataByNumber", self, Metadata, block_number)
    }

    async fn get_current_metadata(&self) -> Response<Metadata> {
        jsonrpc!("axon_getCurrentMetadata", self, Metadata)
    }

    async fn get_ckb_related_info(&self) -> Response<CkbRelatedInfo> {
        jsonrpc!("axon_getCkbRelatedInfo", self, CkbRelatedInfo)
    }
}
