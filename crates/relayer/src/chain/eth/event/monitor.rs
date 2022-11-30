use std::sync::Arc;

use crossbeam_channel as channel;
use ethers::prelude::{abigen, Provider, SignerMiddleware, StreamExt, Ws};
use ethers::signers::Wallet;
use ethers::types::{Address, Log};
use ethers_contract::stream::EventStream;
use ethers_contract::ContractError;
use ethers_providers::FilterWatcher;

use k256::ecdsa::SigningKey;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::debug;

use crate::event::monitor::{MonitorCmd, Next};

type _SubscriptionStream<'a> = EventStream<
    'a,
    FilterWatcher<'a, Ws, Log>,
    IBCEvents,
    ContractError<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>,
>;

type Client = Provider<Ws>;

abigen!(IBC, "./src/chain/eth/IBC.json");

#[derive(Clone, Debug)]
pub struct EthEventMonitor {
    client: Client,
    // event_queries: Vec<IBC<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>>,
    // _subscription: Box<SubscriptionStream<'a>>,
    rt: Arc<TokioRuntime>,
    _chain_id: u64,
    address: Address,
    wallet: Wallet<SigningKey>,
    start_block_number: u64,
    rx_cmd: channel::Receiver<MonitorCmd>,
}

impl EthEventMonitor {
    // pub fn new(
    //     chain_id: ChainId,
    //     node_addr: Url,
    //     rt: Arc<TokioRuntime>,
    // ) -> Result<(Self, EventReceiver, TxMonitorCmd)> {
    //     todo!()
    // }

    // pub fn queries(&self) -> &[Contract<Provider<Ws>>] {
    //     &self.event_queries
    // }

    // pub fn subscribe(&mut self) -> Result<()> {
    //     let signer = SignerMiddleware::new(self.client.clone(), self.wallet.clone());
    //     let ibc = Arc::new(IBC::new(self.address, Arc::new(signer)));
    //     let ibc_events = ibc.events().from_block(self.start_block_number);

    //     // let mut subscriptions = vec![];
    //     let subscription = self
    //         .rt
    //         .block_on(ibc_events.stream())
    //     .map_err(|_| Error::collect_events_failed("fail".to_string()))?;
    //     self.subscription = Box::new(subscription);

    //     Ok(())
    // }

    #[allow(clippy::while_let_loop)]
    pub fn run(mut self) {
        debug!("starting event monitor");
        loop {
            match self.run_loop() {
                Next::Continue => continue,
                Next::Abort => break,
            }
        }
        debug!("event monitor is shutting down");
        // TODO: close client
    }

    #[allow(clippy::while_let_loop)]
    pub fn run_loop(&mut self) -> Next {
        let signer = SignerMiddleware::new(self.client.clone(), self.wallet.clone());
        let ibc = Arc::new(IBC::new(self.address, Arc::new(signer)));
        let ibc_events = Arc::clone(&ibc);

        if let Ok(MonitorCmd::Shutdown) = self.rx_cmd.try_recv() {
            return Next::Abort;
        }
        let rt = self.rt.clone();
        rt.block_on(async move {
            if let Ok(stream) = ibc_events
                .events()
                .from_block(self.start_block_number)
                .stream()
                .await
            {
                let mut meta_stream = stream.with_meta();
                while let Some(Ok((event, meta))) = meta_stream.next().await {
                    if meta.block_number > self.start_block_number.into() {
                        // TODO: convert eth event to IBC Event
                        // TODO: send msg to channel
                        println!("[event] = {:?}", event);
                        println!("[event_meta] = {:?}\n", meta);
                    }
                }
            }
        });
        return Next::Continue;
    }
}
