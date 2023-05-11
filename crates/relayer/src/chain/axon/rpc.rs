use crate::error::Error;

use async_trait::async_trait;
use axon_tools::types::{AxonBlock, Proof, Validator};
use ethers::types::BlockNumber;
use reqwest::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tendermint_rpc::{Error as TmError, Url};

pub type Response<T> = Result<T, Error>;

#[async_trait]
pub trait AxonRpc {
    async fn get_block_by_id(&self, number: BlockNumber) -> Response<AxonBlock>;

    async fn get_proof_by_id(&self, number: BlockNumber) -> Response<Proof>;

    async fn get_validators_by_id(&self, number: BlockNumber) -> Response<Vec<Validator>>;
}

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
    }}
}

#[async_trait]
impl AxonRpc for AxonRpcClient {
    async fn get_block_by_id(&self, number: BlockNumber) -> Response<AxonBlock> {
        jsonrpc!("axon_getBlockById", self, AxonBlock, number)
    }

    async fn get_proof_by_id(&self, number: BlockNumber) -> Response<Proof> {
        jsonrpc!("axon_getProofById", self, Proof, number)
    }

    async fn get_validators_by_id(&self, number: BlockNumber) -> Response<Vec<Validator>> {
        jsonrpc!("axon_getValidatorsById", self, Vec<Validator>, number)
    }
}
