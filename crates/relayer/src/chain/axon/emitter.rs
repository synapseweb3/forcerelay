use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use ckb_jsonrpc_types::{CellInfo, HeaderView, OutPoint};
use ckb_types::H256;
use emitter_core::{
    cell_process::CellProcess,
    header_sync::HeaderSyncProcess,
    rpc_client::RpcClient,
    types::{IndexerTip, RpcSearchKey},
    Submit, SubmitProcess, TipState,
};
use ethers::prelude::{Address, H160};
use ethers::providers::Middleware;
use ethers::types::transaction::eip2718::TypedTransaction::Legacy;
use ethers::{abi::AbiEncode, types::TransactionRequest};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::event::monitor::Error;
use crate::ibc_contract::{ckb_light_client, image_cell};

use super::ContractProvider;

const fn system_contract_address(addr: u8) -> Address {
    H160([
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, addr,
    ])
}

const IMAGE_CELL_ADDRESS: Address = system_contract_address(0x3);
const CKB_LIGHT_CLIENT_ADDRESS: Address = system_contract_address(0x4);

struct CkbTipState(IndexerTip);

impl CkbTipState {
    pub fn new(old_tip_hash: H256, old_tip_number: u64) -> Self {
        let tip = IndexerTip {
            block_hash: old_tip_hash,
            block_number: old_tip_number.into(),
        };
        Self(tip)
    }
}

impl TipState for CkbTipState {
    fn load(&self) -> &IndexerTip {
        &self.0
    }

    fn update(&mut self, current: IndexerTip) {
        self.0 = current;
    }
}

#[derive(Clone)]
struct CkbSubmitProcess {
    chain_id: u64,
    contract: Arc<ContractProvider>,
}

impl CkbSubmitProcess {
    fn new(chain_id: u64, contract: Arc<ContractProvider>) -> Self {
        Self { chain_id, contract }
    }

    fn generate_data(&self, data: Vec<Submit>) -> Vec<u8> {
        let blocks = data
            .into_iter()
            .map(|block| image_cell::BlockUpdate {
                block_number: block.header.inner.number.into(),
                tx_inputs: block.inputs.into_iter().map(Into::into).collect(),
                tx_outputs: self.convert_outputs(block.outputs),
            })
            .collect();
        image_cell::UpdateCall { blocks }.encode()
    }

    fn convert_outputs(&self, outputs: Vec<(OutPoint, CellInfo)>) -> Vec<image_cell::CellInfo> {
        outputs
            .into_iter()
            .map(|(outpoint, cell)| {
                let type_ = if let Some(type_) = cell.output.type_ {
                    vec![type_.into()]
                } else {
                    vec![]
                };
                let data = if let Some(data) = cell.data {
                    data.content.into_bytes().into()
                } else {
                    Default::default()
                };
                image_cell::CellInfo {
                    out_point: outpoint.into(),
                    output: image_cell::CellOutput {
                        capacity: cell.output.capacity.into(),
                        lock: cell.output.lock.into(),
                        type_,
                    },
                    data,
                }
            })
            .collect()
    }

    async fn upload_cells(&self, cells: Vec<Submit>) -> eyre::Result<()> {
        let from = self.contract.address();
        let nonce = self.contract.get_transaction_count(from, None).await?;
        let data = self.generate_data(cells);

        let transaction_request = TransactionRequest::new()
            .chain_id(self.chain_id)
            .to(IMAGE_CELL_ADDRESS)
            .data(data)
            .from(from)
            .gas_price(1)
            .gas(21000)
            .nonce(nonce);

        let tx = Legacy(transaction_request);
        let signature = self.contract.sign_transaction(&tx, from).await?;

        self.contract
            .send_raw_transaction(tx.rlp_signed(&signature))
            .await?
            .await?
            .expect("failed to send to image_cell_contract");

        Ok(())
    }

    async fn upload_headers(&self, headers: Vec<HeaderView>) -> eyre::Result<()> {
        let data = ckb_light_client::UpdateCall {
            headers: headers.into_iter().map(Into::into).collect(),
        }
        .encode();

        let from = self.contract.address();
        let nonce = self.contract.get_transaction_count(from, None).await?;

        let transaction_request = TransactionRequest::new()
            .chain_id(self.chain_id)
            .to(CKB_LIGHT_CLIENT_ADDRESS)
            .data(data)
            .from(from)
            .gas_price(1)
            .gas(21000)
            .nonce(nonce);

        let tx = Legacy(transaction_request);
        let signature = self.contract.sign_transaction(&tx, from).await?;

        self.contract
            .send_raw_transaction(tx.rlp_signed(&signature))
            .await?
            .await?
            .expect("failed to send to light_client_contract");

        Ok(())
    }
}

#[async_trait]
impl SubmitProcess for CkbSubmitProcess {
    fn is_closed(&self) -> bool {
        false
    }

    async fn submit_cells(&mut self, cells: Vec<Submit>) -> bool {
        if let Err(err) = self.upload_cells(cells).await {
            tracing::error!("failed to sync CKB cells: {err}");
        }
        true
    }

    async fn submit_headers(&mut self, headers: Vec<HeaderView>) -> bool {
        if let Err(err) = self.upload_headers(headers).await {
            tracing::error!("failed to sync CKB headers: {err}");
        }
        true
    }
}

pub struct CkbSyncManager {
    rt: Arc<Runtime>,
    rpc: RpcClient,
    chain_id: u64,
    contract: Arc<ContractProvider>,
    start_tip_number: u64,
    cell_processors: HashMap<RpcSearchKey, JoinHandle<()>>,
    header_processor: Option<JoinHandle<()>>,
}

impl CkbSyncManager {
    pub fn new(
        rt: Arc<Runtime>,
        ckb_uri: &str,
        chain_id: u64,
        contract: Arc<ContractProvider>,
        start_tip_number: u64,
    ) -> Self {
        Self {
            rt,
            rpc: RpcClient::new(ckb_uri),
            chain_id,
            contract,
            start_tip_number,
            cell_processors: HashMap::new(),
            header_processor: None,
        }
    }

    fn tip_state(&self, block_number: u64) -> Result<CkbTipState, Error> {
        let old_tip_header = self
            .rt
            .block_on(self.rpc.get_header_by_number(block_number.into()))
            .map_err(|e| {
                Error::others(format!("failed to fetch CKB header {}: {e}", block_number))
            })?;
        let tip_state = CkbTipState::new(old_tip_header.hash, block_number);
        Ok(tip_state)
    }

    pub fn spawn_cell_processor(&mut self, search_key: RpcSearchKey) -> Result<bool, Error> {
        // feature closed
        if self.start_tip_number == 0 {
            return Ok(true);
        }
        if self.cell_processors.contains_key(&search_key) {
            return Ok(false);
        }
        let mut cell_processor = CellProcess::new(
            search_key.clone(),
            self.tip_state(self.start_tip_number)?,
            self.rpc.clone(),
            CkbSubmitProcess::new(self.chain_id, self.contract.clone()),
        );
        let handle = self.rt.spawn(async move {
            cell_processor.run().await;
        });
        self.cell_processors.insert(search_key, handle);
        Ok(true)
    }

    pub fn remove_cell_processor(&mut self, search_key: RpcSearchKey) -> bool {
        if let Some(handle) = self.cell_processors.remove(&search_key) {
            handle.abort();
            true
        } else {
            false
        }
    }

    pub fn spawn_header_processor(&mut self, start_block_number: u64) -> Result<bool, Error> {
        // feature closed
        if start_block_number == 0 {
            return Ok(true);
        }
        if self.header_processor.is_some() {
            return Ok(false);
        }
        let mut header_processor = HeaderSyncProcess::new(
            self.tip_state(start_block_number)?,
            self.rpc.clone(),
            CkbSubmitProcess::new(self.chain_id, self.contract.clone()),
        );
        let handle = self.rt.spawn(async move {
            header_processor.run().await;
        });
        self.header_processor = Some(handle);
        Ok(true)
    }

    // TODO: use this method before each submit of IBC messages
    pub fn _sync_header(&self, headers: Vec<HeaderView>) -> Result<(), Error> {
        self.rt
            .block_on(
                CkbSubmitProcess::new(self.chain_id, self.contract.clone()).upload_headers(headers),
            )
            .map_err(|err| Error::others(err.to_string()))
    }
}
