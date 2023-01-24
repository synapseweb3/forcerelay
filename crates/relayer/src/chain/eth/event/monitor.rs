use std::sync::Arc;

use crate::event::bus::EventBus;
use crate::event::IbcEventWithHeight;
use crossbeam_channel as channel;
use ibc_relayer_types::clients::ics07_eth::header::Header as EthHeader;

use ibc_relayer_types::core::ics02_client::events;

use ibc_relayer_types::Height;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::chain::tracking::TrackingId;
use crate::event::monitor::{EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, info, instrument, warn};

// #[derive(Clone, Debug)]
pub struct EthEventMonitor {
    rt: Arc<TokioRuntime>,
    chain_id: ChainId,
    rx_cmd: channel::Receiver<MonitorCmd>,
    header_receiver: UnboundedReceiver<Vec<EthHeader>>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
}

impl EthEventMonitor {
    /// Create an event monitor, and connect to a node
    #[instrument(
        name = "eth_event_monitor.create",
        level = "error",
        skip_all,
        fields(chain = %chain_id)
    )]
    pub fn new(
        chain_id: ChainId,
        header_receiver: UnboundedReceiver<Vec<EthHeader>>,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, TxMonitorCmd)> {
        let (tx_cmd, rx_cmd) = channel::unbounded();

        let event_bus = EventBus::new();
        let monitor = Self {
            rt,
            chain_id,
            rx_cmd,
            header_receiver,
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
        if let Ok(cmd) = self.rx_cmd.try_recv() {
            match cmd {
                MonitorCmd::Shutdown => return Next::Abort,
                MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
            }
        }

        if let Ok(headers) = self.header_receiver.try_recv() {
            if !headers.is_empty() {
                let start_slot = headers.first().unwrap().slot;
                let end_slot = headers.last().unwrap().slot;
                info!("receive new headers [{}, {}]", start_slot, end_slot);
                let events = headers
                    .into_iter()
                    .map(|header| {
                        let height = Height::new(0, header.slot).unwrap();
                        IbcEventWithHeight::new(events::NewBlock::new(height).into(), height)
                    })
                    .collect();
                let batch = EventBatch {
                    chain_id: self.chain_id.clone(),
                    tracking_id: TrackingId::new_uuid(),
                    height: Height::new(0, end_slot).unwrap(),
                    events,
                };
                self.process_batch(batch);
            } else {
                warn!("receive empty headers");
            }
        }
        Next::Continue
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}
