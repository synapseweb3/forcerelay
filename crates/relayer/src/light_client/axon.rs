#![allow(dead_code, unused_variables, unused_imports)]

use std::sync::Arc;

use ibc_relayer_types::clients::ics07_axon::header::Header;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::chain::axon::AxonChain;
use crate::config::axon::AxonChainConfig;
use crate::error::Error;

pub struct LightClient {
    pub chain_id: ChainId,
}

impl LightClient {
    pub fn from_config(config: &AxonChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error> {
        todo!()
    }

    pub fn subscribe(&mut self) -> (UnboundedReceiver<Header>, UnboundedReceiver<Vec<Header>>) {
        todo!();
    }

    pub fn bootstrap(&mut self) -> Result<(), Error> {
        todo!()
    }
}

// TO IMPLEMENT
impl super::LightClient<AxonChain> for LightClient {
    fn header_and_minimal_set(
        &mut self,
        trusted: ibc_relayer_types::Height,
        target: ibc_relayer_types::Height,
        client_state: &crate::client_state::AnyClientState,
    ) -> Result<super::Verified<<AxonChain as crate::chain::endpoint::ChainEndpoint>::Header>, Error>
    {
        todo!()
    }

    fn verify(
        &mut self,
        trusted: ibc_relayer_types::Height,
        target: ibc_relayer_types::Height,
        client_state: &crate::client_state::AnyClientState,
    ) -> Result<
        super::Verified<<AxonChain as crate::chain::endpoint::ChainEndpoint>::LightBlock>,
        Error,
    > {
        todo!()
    }

    fn check_misbehaviour(
        &mut self,
        update: &ibc_relayer_types::core::ics02_client::events::UpdateClient,
        client_state: &crate::client_state::AnyClientState,
    ) -> Result<Option<crate::misbehaviour::MisbehaviourEvidence>, Error> {
        todo!()
    }

    fn fetch(
        &mut self,
        height: ibc_relayer_types::Height,
    ) -> Result<<AxonChain as crate::chain::endpoint::ChainEndpoint>::LightBlock, Error> {
        todo!()
    }
}
