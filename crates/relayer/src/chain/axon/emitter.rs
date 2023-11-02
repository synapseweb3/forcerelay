use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use ckb_jsonrpc_types::HeaderView;
use ckb_types::H256;
use emitter_core::{
    cell_process::CellProcess,
    rpc_client::RpcClient,
    types::{IndexerTip, RpcSearchKey},
    Submit, SubmitProcess, TipState,
};
use ethers::providers::{Provider, Ws};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::event::monitor::Error;

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
struct CkbSubmitProcess(Arc<Provider<Ws>>);

impl CkbSubmitProcess {
    pub fn new(client: Arc<Provider<Ws>>) -> Self {
        Self(client)
    }
}

#[async_trait]
impl SubmitProcess for CkbSubmitProcess {
    fn is_closed(&self) -> bool {
        false
    }

    async fn submit_cells(&mut self, _cells: Vec<Submit>) -> bool {
        true
    }

    async fn submit_headers(&mut self, _headers: Vec<HeaderView>) -> bool {
        true
    }
}

pub struct CellProcessManager {
    rt: Arc<Runtime>,
    rpc: RpcClient,
    contract: Arc<Provider<Ws>>,
    start_tip_number: u64,
    cell_processors: HashMap<RpcSearchKey, JoinHandle<()>>,
}

impl CellProcessManager {
    pub fn new(
        rt: Arc<Runtime>,
        ckb_uri: &str,
        contract: Arc<Provider<Ws>>,
        start_tip_number: u64,
    ) -> Self {
        Self {
            rt,
            rpc: RpcClient::new(ckb_uri),
            contract,
            start_tip_number,
            cell_processors: HashMap::new(),
        }
    }

    pub fn spawn_cell_processor(&mut self, search_key: &str) -> Result<bool, Error> {
        // feature closed
        if self.start_tip_number == 0 {
            return Ok(true);
        }
        let search_key = serde_json::from_str(search_key)
            .map_err(|e| Error::others(format!("failed to parse search_key from JSON: {e}")))?;
        if self.cell_processors.contains_key(&search_key) {
            return Ok(false);
        }
        let old_tip_header = self
            .rt
            .block_on(self.rpc.get_header_by_number(self.start_tip_number.into()))
            .map_err(|e| {
                Error::others(format!(
                    "failed to fetch CKB header {}: {e}",
                    self.start_tip_number
                ))
            })?;
        let tip_state = CkbTipState::new(old_tip_header.hash, self.start_tip_number);
        let mut cell_processor = CellProcess::new(
            search_key.clone(),
            tip_state,
            self.rpc.clone(),
            CkbSubmitProcess::new(self.contract.clone()),
        );
        let handle = self.rt.spawn(async move {
            cell_processor.run().await;
        });
        self.cell_processors.insert(search_key, handle);
        Ok(true)
    }

    pub fn remove_cell_processor(&mut self, search_key: &str) -> Result<bool, Error> {
        let search_key = serde_json::from_str(search_key)
            .map_err(|e| Error::others(format!("failed to parse search_key from JSON: {e}")))?;
        if let Some(handle) = self.cell_processors.remove(&search_key) {
            handle.abort();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
