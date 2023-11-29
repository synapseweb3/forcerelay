use crate::error::Error;

use async_trait::async_trait;
use ckb_ics_axon::axon_tools::types::{Block as AxonBlock, CkbRelatedInfo, Metadata, Proof};
use ethers::types::{BlockId, BlockNumber, Bytes, H160, U256};
use reqwest::Client;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tendermint_rpc::Url;

pub type Response<T> = Result<T, Error>;

#[async_trait]
pub trait AxonRpc {
    async fn get_block_by_id(&self, block_id: BlockId) -> Response<Option<AxonBlock>>;

    async fn get_proof_by_id(&self, block_id: BlockId) -> Response<Option<Proof>>;

    async fn get_metadata_by_number(&self, block_number: BlockNumber) -> Response<Metadata>;

    async fn get_current_metadata(&self) -> Response<Metadata>;

    async fn get_ckb_related_info(&self) -> Response<CkbRelatedInfo>;

    /// ethers StorageProof key is wrong so we define our own.
    async fn eth_get_proof(
        &self,
        address: H160,
        positions: Vec<U256>,
        block_id: Option<BlockId>,
    ) -> Response<EIP1186ProofResponse>;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EIP1186ProofResponse {
    pub account_proof: Vec<Bytes>,
    pub storage_proof: Vec<StorageProof>,
}

#[derive(Deserialize)]
pub struct StorageProof {
    pub key: U256,
    pub value: U256,
    pub proof: Vec<Bytes>,
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
    async fn get_block_by_id(&self, block_id: BlockId) -> Response<Option<AxonBlock>> {
        jsonrpc!("axon_getBlockById", self, Option<AxonBlock>, block_id)
    }

    async fn get_proof_by_id(&self, block_id: BlockId) -> Response<Option<Proof>> {
        jsonrpc!("axon_getProofById", self, Option<Proof>, block_id)
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

    async fn eth_get_proof(
        &self,
        address: H160,
        positions: Vec<U256>,
        block_id: Option<BlockId>,
    ) -> Response<EIP1186ProofResponse> {
        jsonrpc!(
            "eth_getProof",
            self,
            EIP1186ProofResponse,
            address,
            positions,
            block_id
        )
    }
}
