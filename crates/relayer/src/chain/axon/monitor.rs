use std::sync::Arc;
use std::time::Duration;

use super::contract::*;
use crate::event::bus::EventBus;
use crate::event::IbcEventWithHeight;
use crossbeam_channel as channel;
use ethers::contract::LogMeta;
use ethers::prelude::*;
use ethers::providers::Middleware;
use ethers::types::Address;
use ibc_relayer_types::Height;
use OwnableIBCHandler as Contract;
use OwnableIBCHandlerEvents as ContractEvents;

use crate::chain::tracking::TrackingId;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tendermint_rpc::WebSocketClientUrl;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, error, info, instrument, warn};

type Client = Provider<Ws>;

// #[derive(Clone, Debug)]
pub struct AxonEventMonitor {
    websocket_addr: WebSocketClientUrl,
    client: Arc<Client>,
    rt: Arc<TokioRuntime>,
    chain_id: ChainId,
    contract_address: Address,
    start_block_number: u64,
    rx_cmd: channel::Receiver<MonitorCmd>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
}

impl AxonEventMonitor {
    /// Create an event monitor, and connect to a node
    #[instrument(
        name = "axon_event_monitor.create",
        level = "error",
        skip_all,
        fields(chain = %chain_id, addr = %websocket_addr)
    )]
    pub fn new(
        chain_id: ChainId,
        websocket_addr: WebSocketClientUrl,
        contract_address: Address,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, TxMonitorCmd)> {
        let (tx_cmd, rx_cmd) = channel::unbounded();

        let client = rt
            .block_on(Provider::<Ws>::connect(websocket_addr.to_string()))
            .map_err(|_| Error::client_creation_failed(chain_id.clone(), websocket_addr.clone()))?;

        let start_block_number = rt
            .block_on(client.get_block_number())
            .map_err(|e| Error::others(e.to_string()))?
            .as_u64();

        let event_bus = EventBus::new();
        let monitor = Self {
            websocket_addr,
            client: Arc::new(client),
            rt,
            chain_id,
            contract_address,
            start_block_number,
            rx_cmd,
            event_bus,
        };
        Ok((monitor, TxMonitorCmd::new(tx_cmd)))
    }

    // XXX: we met a connection error that ethers-rs doesn't reconnect WebSocket if it meets error,
    //      we just choose to recreate provider mannully to solve connection problem
    //
    //      see: https://github.com/gakonst/ethers-rs/issues/2323
    fn new_ws_provider(&mut self) -> Result<Client> {
        let client = self
            .rt
            .block_on(Provider::<Ws>::connect(self.websocket_addr.to_string()))
            .map_err(|_| {
                Error::client_creation_failed(self.chain_id.clone(), self.websocket_addr.clone())
            })?;
        Ok(client)
    }

    #[allow(clippy::while_let_loop)]
    #[instrument(
        name = "axon_event_monitor",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    pub fn run(mut self) {
        if let Next::Continue = self.update_subscribe(false) {
            info!("start Axon event monitor for {}", self.chain_id,);
            let mut contract = Contract::new(self.contract_address, Arc::clone(&self.client));
            info!(
                "start to fetch IBC events from block {}",
                self.start_block_number
            );
            loop {
                std::thread::sleep(Duration::from_secs(1));
                match self.run_once(&contract) {
                    (Next::Abort, _) => break,
                    (Next::Continue, false) => match self.new_ws_provider() {
                        Ok(client) => {
                            // recreate contract when WS connection meets error
                            self.client = Arc::new(client);
                            contract =
                                Contract::new(self.contract_address, Arc::clone(&self.client));
                            info!(
                                "restart to fetch IBC events from block {}",
                                self.start_block_number
                            );
                        }
                        Err(err) => {
                            error!("restart provider failed: {err}");
                        }
                    },
                    (Next::Continue, true) => {}
                }
            }
            debug!("event monitor is shutting down");
        }
    }

    fn update_subscribe(&mut self, use_try: bool) -> Next {
        let cmd = if use_try {
            match self.rx_cmd.try_recv() {
                Ok(cmd) => cmd,
                Err(e) if e.is_disconnected() => return Next::Abort,
                // No command yet.
                Err(_) => return Next::Continue,
            }
        } else {
            match self.rx_cmd.recv() {
                Ok(cmd) => cmd,
                // Disconnected.
                Err(_) => return Next::Abort,
            }
        };
        match cmd {
            MonitorCmd::Shutdown => return Next::Abort,
            MonitorCmd::Subscribe(tx) => {
                if let Err(e) = tx.send(self.event_bus.subscribe()) {
                    error!("failed to send back subscription: {e}");
                }
            }
        }
        Next::Continue
    }

    fn run_once(&mut self, contract: &OwnableIBCHandler<Client>) -> (Next, bool) {
        if let Next::Abort = self.update_subscribe(true) {
            return (Next::Abort, true);
        }

        let tip_block_number = match self.rt.block_on(contract.client().get_block_number()) {
            Ok(tip) => tip.as_u64(),
            Err(err) => {
                error!("failed to fetch Axon latest block number: {err}");
                return (Next::Continue, false);
            }
        };

        if self.start_block_number >= tip_block_number {
            return (Next::Continue, true);
        }

        let query = contract
            .events()
            .from_block(self.start_block_number)
            .to_block(tip_block_number);
        let events = match self.rt.block_on(query.query_with_meta()) {
            Ok(events) => events,
            Err(err) => {
                error!(
                    "failed to fetch events from block {} to block {tip_block_number}: {err}",
                    self.start_block_number
                );
                return (Next::Continue, false);
            }
        };

        events
            .into_iter()
            .for_each(|(event, meta)| self.process_event(event, meta));

        self.start_block_number = tip_block_number + 1;
        (Next::Continue, true)
    }

    fn process_event(&mut self, event: ContractEvents, meta: LogMeta) {
        println!("\n{}\n[event] = {:?}", self.chain_id, event);
        println!("[event_meta] = {:?}\n", meta);

        self.start_block_number = meta.block_number.as_u64();
        let event = IbcEventWithHeight::new_with_tx_hash(
            event.into(),
            Height::from_noncosmos_height(meta.block_number.as_u64()),
            meta.transaction_hash.into(),
        );
        let batch = EventBatch {
            chain_id: self.chain_id.clone(),
            tracking_id: TrackingId::Static("Axon solidity event streaming"),
            height: Height::from_noncosmos_height(meta.block_number.as_u64()),
            events: vec![event],
        };
        self.process_batch(batch);
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}
