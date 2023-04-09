use std::sync::Arc;

use super::contract::*;
// use super::ibc::*;
use crate::event::bus::EventBus;
use crate::event::IbcEventWithHeight;
use crate::light_client::AnyHeader;
use crossbeam_channel as channel;
use ethers::prelude::*;
use ethers::prelude::{Provider, StreamExt, Ws};
use ethers::types::Address;
use ethers_contract::LogMeta;
use ethers_providers::Middleware;
use ibc_relayer_types::clients::ics07_axon::header::Header as AxonHeader;
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::events::{self, Attributes};
use ibc_relayer_types::core::ics02_client::header::Header;
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::Height;
use tokio::sync::mpsc::UnboundedReceiver;
use OwnableIBCHandler as Contract;
use OwnableIBCHandlerEvents as ContractEvents;

use crate::chain::tracking::TrackingId;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};
use tendermint_rpc::Url;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, error, info, instrument};

type Client = Provider<Ws>;
// abigen!(IBC, "./crates/relayer/src/chain/axon/IBC.json");
// use IBCEvents as ContractIBCEvents;

// #[derive(Clone, Debug)]
pub struct AxonEventMonitor {
    client: Arc<Client>,
    rt: Arc<TokioRuntime>,
    chain_id: ChainId,
    contract_address: Address,
    start_block_number: u64,
    rx_cmd: channel::Receiver<MonitorCmd>,
    header_receiver: UnboundedReceiver<Vec<AxonHeader>>,
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
        websocket_addr: Url,
        contract_address: Address,
        header_receiver: UnboundedReceiver<Vec<AxonHeader>>,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, TxMonitorCmd)> {
        let (tx_cmd, rx_cmd) = channel::unbounded();

        // let ws_addr = websocket_addr.clone();
        let client = rt
            .block_on(Provider::<Ws>::connect(websocket_addr.to_string()))
            .map_err(|_| Error::client_creation_failed(chain_id.clone(), websocket_addr))?;

        let start_block_number = rt
            .block_on(client.get_block_number())
            .map_err(|e| Error::others(e.to_string()))?;

        let event_bus = EventBus::new();
        let monitor = Self {
            client: Arc::new(client),
            rt,
            chain_id,
            contract_address,
            start_block_number: start_block_number.as_u64(),
            rx_cmd,
            header_receiver,
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
        debug!("starting event monitor");
        let rt = self.rt.clone();
        rt.block_on(async {
            loop {
                match self.run_loop().await {
                    Next::Continue => continue,
                    Next::Abort => break,
                }
            }
        });
        debug!("event monitor is shutting down");
        // TODO: close client
    }

    async fn run_loop(&mut self) -> Next {
        if let Ok(cmd) = self.rx_cmd.try_recv() {
            match cmd {
                MonitorCmd::Shutdown => return Next::Abort,
                MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
            }
        }

        let contract = Arc::new(Contract::new(
            self.contract_address,
            Arc::clone(&self.client),
        ));
        let events = contract.events().from_block(self.start_block_number);
        if let Ok(stream) = events.stream().await {
            let mut meta_stream = stream.with_meta();
            loop {
                tokio::select! {
                    Some(headers) = self.header_receiver.recv() => {
                        if let (Some(first), Some(last)) = (headers.first(), headers.last()) {
                            info!("receive a new header: [{:?}, {:?}]", headers.first(), headers.last());
                            let events = headers.iter()
                                .map(|header| {
                                    let height = header.height();
                                    IbcEventWithHeight::new(
                                        events::NewBlock::new(height).into(),
                                        height,
                                    )})
                                .collect();
                            let batch = EventBatch {
                                chain_id: self.chain_id.clone(),
                                tracking_id: TrackingId::new_uuid(),
                                height: last.height(),
                                events,
                            };
                            self.process_batch(batch);
                        }
                    },

                    Some(ret) = meta_stream.next() => {
                        if let Ok(cmd) = self.rx_cmd.try_recv() {
                            match cmd {
                                MonitorCmd::Shutdown => return Next::Abort,
                                MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
                            }
                        }
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
                }
            }
        }
        Next::Continue
    }

    fn process_event(&mut self, event: ContractEvents, meta: LogMeta) -> Result<()> {
        info!("[event] = {:?}", event);
        info!("[event_meta] = {:?}\n", meta);
        let batch = EventBatch {
            chain_id: self.chain_id.clone(),
            tracking_id: TrackingId::new_uuid(),
            height: Height::new(0, meta.block_number.as_u64()).unwrap(),
            events: vec![self.to_ibc_event(event, meta.block_number.as_u64())],
        };
        self.process_batch(batch);
        self.start_block_number = meta.block_number.as_u64();
        Ok(())
    }

    fn to_ibc_event(&self, event: ContractEvents, height: u64) -> IbcEventWithHeight {
        let attr = Attributes::default();
        let ibc_event = match event {
            ContractEvents::GeneratedClientIdentifierFilter(event) => {
                info!("GeneratedClientIdentifierFilter: {:?}", event.0);
                let attr = Attributes {
                    client_id: event.0.parse().unwrap(),
                    client_type: ClientType::Axon,
                    consensus_height: Height::new(0, height).unwrap(),
                };
                IbcEvent::CreateClient(events::CreateClient(attr))
            }
            _ => todo!(),
        };
        IbcEventWithHeight::new(ibc_event, Height::new(0, height).unwrap())
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}
