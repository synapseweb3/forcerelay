#![allow(dead_code, unused_variables, unused_imports)]

use std::sync::Arc;

use ethers::prelude::k256::ecdsa::SigningKey;
use ethers::prelude::*;
use futures::TryFutureExt;
use ibc_relayer_types::clients::ics07_axon::{
    header::Header, light_block::LightBlock as AxonLightBlock,
};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;
use tracing::info;

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
        let emitters = self.header_updaters.clone();
        self.rt.spawn(async move {
            info!("axon: start watching new block from axon chain");
            let mut stream = provider
                .watch_blocks()
                .await
                .expect("unsupported ws client");
            while let Some(block_hash) = stream.next().await {
                let block = rpc
                    .get_block_by_id(block_hash.into())
                    .await
                    .expect("watch axon block failed");
                info!(
                    "axon: new block {} with hash {:?}",
                    block.header.number, block_hash
                );
                // axon validators would refresh in case of the change of epoch
                if block.header.number % epoch_len == 0 {
                    info!(
                        "axon: new epoch starting with block {}",
                        block.header.number
                    );
                    for emitter in emitters.read().await.iter() {
                        let header = Header {
                            axon_header: block.header.clone(),
                        };
                        emitter.send(header).await.expect("send axon header");
                    }
                }
            }
            info!("axon: end watching new block from axon chain");
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
    ) -> Result<Verified<AxonLightBlock>, Error> {
        Ok(Verified {
            target: AxonLightBlock::default(),
            supporting: vec![],
        })
    }

    fn check_misbehaviour(
        &mut self,
        update: &ibc_relayer_types::core::ics02_client::events::UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        Ok(None)
    }

    fn fetch(&mut self, height: ibc_relayer_types::Height) -> Result<AxonLightBlock, Error> {
        todo!()
    }
}
