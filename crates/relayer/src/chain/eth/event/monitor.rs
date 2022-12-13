use std::sync::Arc;

use super::ibc::*;
use crate::event::bus::EventBus;
use crate::event::IbcEventWithHeight;
use crate::light_client::AnyHeader;
use crossbeam_channel as channel;
use ethers::prelude::{Provider, StreamExt, Ws};
use ethers::types::Address;
use ethers_contract::LogMeta;
use ethers_providers::Middleware;
use ibc_relayer_types::clients::ics07_eth::header::Update as EthHeader;
use ibc_relayer_types::core::ics02_client::events::{self, Attributes};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::Height;

use crate::chain::tracking::TrackingId;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tendermint_rpc::Url;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, error, instrument};

type Client = Provider<Ws>;

// #[derive(Clone, Debug)]
pub struct EthEventMonitor {
    client: Arc<Client>,
    rt: Arc<TokioRuntime>,
    chain_id: ChainId,
    address: Address,
    start_block_number: u64,
    rx_cmd: channel::Receiver<MonitorCmd>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
}

impl EthEventMonitor {
    /// Create an event monitor, and connect to a node
    #[instrument(
        name = "eth_event_monitor.create",
        level = "error",
        skip_all,
        fields(chain = %chain_id, addr = %node_addr)
    )]
    pub fn new(
        chain_id: ChainId,
        node_addr: Url,
        address: String,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, TxMonitorCmd)> {
        let (tx_cmd, rx_cmd) = channel::unbounded();

        let ws_addr = node_addr.clone();
        let client = rt
            .block_on(Provider::<Ws>::connect(node_addr))
            .map_err(|_| Error::client_creation_failed(chain_id.clone(), ws_addr))?;

        let address = address
            .parse::<Address>()
            .map_err(|e| Error::others(e.to_string()))?;

        let start_block_number = rt
            .block_on(client.get_block_number())
            .map_err(|e| Error::others(e.to_string()))?;

        let event_bus = EventBus::new();
        let monitor = Self {
            client: Arc::new(client),
            rt,
            chain_id,
            address,
            start_block_number: start_block_number.as_u64(),
            rx_cmd,
            event_bus,
        };
        Ok((monitor, TxMonitorCmd::new(tx_cmd)))
    }

    #[allow(clippy::while_let_loop)]
    #[instrument(
        name = "eth_event_monitor",
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
        let ibc = Arc::new(IBC::new(self.address, Arc::clone(&self.client)));
        let ibc_events = Arc::clone(&ibc);

        if let Ok(stream) = ibc_events
            .events()
            .from_block(self.start_block_number)
            .stream()
            .await
        {
            let mut meta_stream = stream.with_meta();

            loop {
                if let Ok(cmd) = self.rx_cmd.try_recv() {
                    match cmd {
                        MonitorCmd::Shutdown => return Next::Abort,
                        MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
                    }
                }
                if let Some(ret) = meta_stream.next().await {
                    if let Ok(MonitorCmd::Shutdown) = self.rx_cmd.try_recv() {
                        return Next::Abort;
                    }
                    match ret {
                        Ok((event, meta)) => {
                            self.process_event(event, meta).unwrap_or_else(|e| {
                                error!("error while process event: {}", e);
                            });
                        }
                        Err(err) => {
                            error!("error when monitoring eth events, reason: {}", err);
                            // TODO: reconnect
                            return Next::Continue;
                        }
                    }
                } else {
                    return Next::Continue;
                }
            }
        }
        Next::Continue
    }

    fn process_event(&mut self, event: IBCEvents, meta: LogMeta) -> Result<()> {
        println!("[event] = {:?}", event);
        println!("[event_meta] = {:?}\n", meta);
        let batch: EventBatch = EventBatch {
            chain_id: self.chain_id.clone(),
            tracking_id: TrackingId::new_uuid(),
            height: Height::new(0, meta.block_number.as_u64()).unwrap(),
            events: vec![self.to_ibc_event(event, meta.block_number.as_u64())],
        };
        self.process_batch(batch);
        self.start_block_number = meta.block_number.as_u64();
        Ok(())
    }

    fn to_ibc_event(&self, event: IBCEvents, height: u64) -> IbcEventWithHeight {
        let attr = Attributes::default();
        let ibc_event = match event {
            IBCEvents::CreateClientFilter(_) => IbcEvent::CreateClient(events::CreateClient(attr)),
            // _ => IbcEvent::CreateClient(events::CreateClient(attr)),
            _ => IbcEvent::UpdateClient(events::UpdateClient {
                common: attr,
                header: Some(Box::new(AnyHeader::Eth(EthHeader::default()))),
            }),
        };
        IbcEventWithHeight {
            event: ibc_event,
            height: Height::new(0, height).unwrap(),
        }
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}
