#![allow(dead_code, unused_variables, unused_imports)]

use std::sync::Arc;

use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::*;
use ethers::prelude::{Provider, Ws};
use futures::TryFutureExt;
use ibc_relayer_types::clients::ics07_axon::header::Header;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;

use crate::chain::axon::{AxonChain, AxonRpc};
use crate::chain::endpoint::ChainEndpoint;
use crate::client_state::AnyClientState;
use crate::config::axon::AxonChainConfig;
use crate::error::Error;
use crate::misbehaviour::MisbehaviourEvidence;

use super::Verified;

pub struct LightClient {
    rt: Arc<TokioRuntime>,
    chain_id: ChainId,
    header_updaters: Arc<RwLock<Vec<Sender<Header>>>>,
}

impl LightClient {
    pub fn from_config(config: &AxonChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error> {
        Ok(Self {
            rt,
            chain_id: config.id.clone(),
            header_updaters: Arc::new(RwLock::new(vec![])),
        })
    }

    pub fn subscribe(&mut self) -> Receiver<Header> {
        let (tx, rx) = channel(1);
        self.rt.block_on(self.header_updaters.write()).push(tx);
        rx
    }

    pub fn bootstrap<T: AxonRpc + Sync + Send + 'static>(
        &self,
        provider: Arc<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>,
        rpc: T,
        epoch_len: u64,
    ) -> Result<(), Error> {
        let rt = self.rt.clone();
        let emiters = self.header_updaters.clone();
        self.rt.spawn(async move {
            provider
                .watch_blocks()
                .await
                .expect("unsupported ws client")
                .for_each(|block_hash| {
                    let block = rt
                        .block_on(rpc.get_block_by_id(block_hash.into()))
                        .expect("watch axon block failed");
                    // axon validators would refresh in case of the change of epoch
                    if block.header.number % epoch_len == 0 {
                        rt.block_on(emiters.read()).iter().for_each(|emiter| {
                            let header = Header {
                                axon_header: block.header.clone(),
                            };
                            rt.block_on(emiter.send(header)).expect("send axon header");
                        });
                    }
                    futures::future::ready(())
                })
                .await;
        });
        Ok(())
    }
}

// TO IMPLEMENT
impl super::LightClient<AxonChain> for LightClient {
    fn header_and_minimal_set(
        &mut self,
        trusted: ibc_relayer_types::Height,
        target: ibc_relayer_types::Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<Header>, Error> {
        todo!()
    }

    fn verify(
        &mut self,
        trusted: ibc_relayer_types::Height,
        target: ibc_relayer_types::Height,
        client_state: &AnyClientState,
    ) -> Result<Verified<<AxonChain as ChainEndpoint>::LightBlock>, Error> {
        todo!()
    }

    fn check_misbehaviour(
        &mut self,
        update: &ibc_relayer_types::core::ics02_client::events::UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        todo!()
    }

    fn fetch(
        &mut self,
        height: ibc_relayer_types::Height,
    ) -> Result<<AxonChain as ChainEndpoint>::LightBlock, Error> {
        todo!()
    }
}
