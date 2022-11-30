use std::sync::Arc;

use ethers::signers::Wallet;
use ethers::types::{Log, Address};
use ethers::prelude::{
    abigen, Provider, SignerMiddleware, StreamExt, Ws,
};
use ethers_contract::stream::EventStream;
use ethers_contract::ContractError;
use ethers_providers::FilterWatcher;

use k256::ecdsa::SigningKey;
use tokio::runtime::Runtime as TokioRuntime;

use crate::event::monitor::Next;

// type SubscriptionStream = EventStream<'static, FilterWatcher<'static, Ws, Log>, EthEventMonitor, ContractError<Client>>;
type SubscriptionStream<'a> = EventStream<
    'a,
    FilterWatcher<'a, Ws, Log>,
    IBCEvents,
    ContractError<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>,
>;

// type Query =
type Client = Provider<Ws>;

abigen!(IBC, "./src/chain/eth/IBC.json");
pub struct EthEventMonitor<'a> {
    client: Client,
    // event_queries: Vec<IBC<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>>,
    _subscription: Box<SubscriptionStream<'a>>,
    rt: Arc<TokioRuntime>,
    _chain_id: u64,
    address: Address,
    wallet: Wallet<SigningKey>,
    start_block_number: u64,
}

impl<'a> EthEventMonitor<'a> {
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


    pub fn run_loop(&mut self) -> Next {
        let signer = SignerMiddleware::new(self.client.clone(), self.wallet.clone());
        let ibc = Arc::new(IBC::new(self.address, Arc::new(signer)));
        let ibc_events = Arc::clone(&ibc);

        let rt = self.rt.clone();
        rt.block_on(async move {
            if let Ok(stream) = ibc_events.events().from_block(self.start_block_number).stream().await {
                let mut meta_stream = stream.with_meta();
                while let Some(Ok((event, meta))) = meta_stream.next().await {
                    if meta.block_number > self.start_block_number.into() {
                        // TODO: convert eth event to IBC Event
                        println!("[event] = {:?}", event);
                        println!("[event_meta] = {:?}\n", meta);
                    }
                }
            }
        });


        todo!()

    }

}
