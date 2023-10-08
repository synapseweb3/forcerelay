use std::sync::Arc;
use std::time::Duration;

use super::contract::*;
use crate::chain::axon::cache_ics_tx_hash_with_event;
use crate::event::bus::EventBus;
use crate::event::IbcEventWithHeight;
use crossbeam_channel as channel;
use ethers::contract::LogMeta;
use ethers::prelude::*;
use ethers::providers::Middleware;
use ethers::types::Address;
use ibc_relayer_types::Height;
use tokio_stream::StreamExt;
use OwnableIBCHandler as Contract;
use OwnableIBCHandlerEvents as ContractEvents;

use crate::chain::tracking::TrackingId;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tendermint_rpc::WebSocketClientUrl;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, error, info, instrument};

type Client = Provider<Ws>;

// #[derive(Clone, Debug)]
pub struct AxonEventMonitor {
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
            .map_err(|_| Error::client_creation_failed(chain_id.clone(), websocket_addr))?;

        // FIXME: here should consider recovering from long-time-crash
        let start_block_number = rt
            .block_on(client.get_block_number())
            .map_err(|e| Error::others(e.to_string()))?
            .as_u64();

        info!("listen IBC events from block {start_block_number}");
        let event_bus = EventBus::new();
        let monitor = Self {
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

    #[allow(clippy::while_let_loop)]
    #[instrument(
        name = "axon_event_monitor",
        level = "error",
        skip_all,
        fields(chain = %self.chain_id)
    )]
    pub fn run(mut self) {
        if let Next::Continue = self.update_subscribe(false) {
            info!("start Axon event monitor for {}", self.chain_id);
            loop {
                if let Next::Abort = self.run_loop() {
                    break;
                }
            }
            debug!("event monitor is shutting down");
        }
        // TODO: close client
    }

    pub fn restore_event_tx_hashes(
        &self,
        latest_block_count: u64,
    ) -> Result<Vec<IbcEventWithHeight>> {
        let contract = Arc::new(Contract::new(
            self.contract_address,
            Arc::clone(&self.client),
        ));
        let restore_block_number = self
            .start_block_number
            .checked_sub(latest_block_count)
            .ok_or(Error::others(format!(
                "latest_block_count {latest_block_count} exceeds start_block_number {}",
                self.start_block_number
            )))?;
        let events = self
            .rt
            .block_on(
                contract
                    .events()
                    .from_block(restore_block_number)
                    .to_block(self.start_block_number)
                    .query_with_meta(),
            )
            .map_err(|e| Error::others(e.to_string()))?
            .into_iter()
            .map(|(event, meta)| {
                IbcEventWithHeight::new_with_tx_hash(
                    event.into(),
                    Height::from_noncosmos_height(meta.block_number.as_u64()),
                    meta.transaction_hash.into(),
                )
            })
            .collect::<Vec<_>>();
        debug!(
            "restored {} events on contract {}",
            events.len(),
            self.contract_address
        );
        Ok(events)
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

    fn run_loop(&mut self) -> Next {
        const TIMEOUT_MILLIS: u64 = 300;

        let contract = Arc::new(Contract::new(
            self.contract_address,
            Arc::clone(&self.client),
        ));
        let events = contract.events().from_block(self.start_block_number);
        if let Ok(mut meta_stream) = self.rt.block_on(async {
            events.stream().await.map(|stream| {
                let meta_stream = stream.with_meta().timeout_repeating(tokio::time::interval(
                    Duration::from_millis(TIMEOUT_MILLIS),
                ));
                meta_stream
            })
        }) {
            debug!("setup IBC contract events streaming process");
            loop {
                if let Next::Abort = self.update_subscribe(true) {
                    return Next::Abort;
                }

                if let Some(Ok(ret)) = self.rt.block_on(meta_stream.next()) {
                    match ret {
                        Ok((event, meta)) => {
                            self.process_event(event, meta).unwrap_or_else(|e| {
                                error!("error while process event: {:?}", e);
                            });
                        }
                        Err(err) => {
                            error!("error when monitoring axon events, reason: {:?}", err);
                            return Next::Continue;
                            // TODO: reconnect
                        }
                    }
                }

                // Some(header) = self.header_receiver.recv() => {
                //     if let Next::Abort = self.update_subscribe() {
                //         return Next::Abort;
                //     }
                //     let height = Height::new(0u64, header.number).expect("axon header height");
                //     let event = IbcEventWithHeight::new(
                //         events::NewBlock::new(height).into(),
                //         height,
                //     );
                //     let batch = EventBatch {
                //         chain_id: self.chain_id.clone(),
                //         tracking_id: TrackingId::new_uuid(),
                //         height,
                //         events: vec![event],
                //     };
                //     self.process_batch(batch);
                // },
            }
        }
        Next::Abort
    }

    fn process_event(&mut self, event: ContractEvents, meta: LogMeta) -> Result<()> {
        println!("\n{}\n[event] = {:?}", self.chain_id, event);
        println!("[event_meta] = {:?}\n", meta);
        self.start_block_number = meta.block_number.as_u64();
        let event = IbcEventWithHeight::new_with_tx_hash(
            event.into(),
            Height::from_noncosmos_height(meta.block_number.as_u64()),
            meta.transaction_hash.into(),
        );
        cache_ics_tx_hash_with_event(self.chain_id.clone(), event.event.clone(), event.tx_hash);
        let batch = EventBatch {
            chain_id: self.chain_id.clone(),
            tracking_id: TrackingId::Static("Axon solidity event streaming"),
            height: Height::from_noncosmos_height(meta.block_number.as_u64()),
            events: vec![event],
        };
        self.process_batch(batch);
        Ok(())
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}
