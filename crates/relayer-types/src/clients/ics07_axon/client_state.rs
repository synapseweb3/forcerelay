use crate::{
    core::{
        ics02_client::{client_state::UpgradeOptions, client_type::ClientType},
        ics24_host::identifier::ChainId,
    },
    prelude::*,
};
use core::convert::TryFrom;
use ibc_proto::google::protobuf::Any;
use serde::{Deserialize, Serialize};

use crate::core::ics02_client::{
    client_state::ClientState as Ics02ClientState, error::Error as Ics02Error,
};
use ibc_proto::protobuf::Protobuf;

pub const CLIENT_STATE_TYPE_URL: &str = "/axon.client.v1.state";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Axon
    }

    fn latest_height(&self) -> crate::Height {
        todo!()
    }

    fn frozen_height(&self) -> Option<crate::Height> {
        None
    }

    fn expired(&self, _elapsed: core::time::Duration) -> bool {
        false
    }

    fn upgrade(
        &mut self,
        _upgrade_height: crate::Height,
        _upgrade_options: &dyn UpgradeOptions,
        _chain_id: ChainId,
    ) {
        todo!()
    }
}

impl Protobuf<Any> for ClientState {}

impl TryFrom<Any> for ClientState {
    type Error = Ics02Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<ClientState> for Any {
    fn from(client: ClientState) -> Self {
        todo!()
    }
}
