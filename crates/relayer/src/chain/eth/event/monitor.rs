use std::sync::Arc;

use crossbeam_channel as channel;
use ethers::prelude::{abigen, Provider, SignerMiddleware, StreamExt, Ws};
use ethers::signers::Wallet;
use ethers::types::{Address, Log};
use ethers_contract::stream::EventStream;
use ethers_contract::{ContractError, LogMeta};
use ethers_providers::FilterWatcher;

use ibc_relayer_types::applications::transfer::events::Event;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use k256::ecdsa::SigningKey;
use tendermint_rpc::Url;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{debug, error};

use crate::event::monitor::{
    Error, EventBatch, EventReceiver, MonitorCmd, Next, Result, TxMonitorCmd,
};

type Client = Provider<Ws>;

abigen!(IBC, "./src/chain/eth/IBC.json");

// #[derive(Clone, Debug)]
pub struct EthEventMonitor {
    client: Arc<Client>,
    // event_queries: Vec<IBC<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>>,
    rt: Arc<TokioRuntime>,
    _chain_id: u64,
    address: Address,
    wallet: Wallet<SigningKey>,
    start_block_number: u64,
    rx_cmd: channel::Receiver<MonitorCmd>,
    tx_batch: channel::Sender<Result<EventBatch>>,
}

impl EthEventMonitor {
    pub fn new(
        chain_id: ChainId,
        node_addr: Url,
        rt: Arc<TokioRuntime>,
    ) -> Result<(Self, EventReceiver, TxMonitorCmd)> {
        todo!()
    }

    // pub fn queries(&self) -> &[Contract<Provider<Ws>>] {
    //     &self.event_queries
    // }

    #[allow(clippy::while_let_loop)]
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

            while let Some(ret) = meta_stream.next().await {
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
                if let Ok(MonitorCmd::Shutdown) = self.rx_cmd.try_recv() {
                    return Next::Abort;
                }
            }
        }
        Next::Continue
    }

    fn process_event(&self, event: IBCEvents, meta: LogMeta) -> Result<()> {
        println!("[event] = {:?}", event);
        println!("[event_meta] = {:?}\n", meta);
        // match event {
        //     IBCEvents::CreateClientFilter(_) => todo!(),
        //     IBCEvents::RoleAdminChangedFilter(_) => todo!(),
        //     IBCEvents::RoleGrantedFilter(_) => todo!(),
        //     IBCEvents::RoleRevokedFilter(_) => todo!(),
        //     IBCEvents::UpdateClientFilter(_) => todo!(),
        // }
        // TODO: convert eth event to IBC Event
        // TODO: send msg to channel
        let batch: EventBatch = todo!();

        self.tx_batch
            .send(Ok(batch))
            .map_err(|_| Error::channel_send_failed())?;
        self.start_block_number = meta.block_number.as_u64();
        Ok(())
    }
}
