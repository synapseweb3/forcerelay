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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        todo!()
    }

    fn client_type(&self) -> ClientType {
        todo!()
    }

    fn latest_height(&self) -> crate::Height {
        todo!()
    }

    fn frozen_height(&self) -> Option<crate::Height> {
        todo!()
    }

    fn expired(&self, _elapsed: core::time::Duration) -> bool {
        todo!()
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

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<ClientState> for Any {
    fn from(_value: ClientState) -> Self {
        todo!()
    }
}
