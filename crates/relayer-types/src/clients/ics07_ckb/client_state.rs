use crate::{
    core::{
        ics02_client::{client_state::UpgradeOptions, client_type::ClientType},
        ics24_host::identifier::ChainId,
    },
    prelude::*,
    Height,
};
use core::convert::TryFrom;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

use crate::core::ics02_client::{
    client_state::ClientState as Ics02ClientState, error::Error as Ics02Error,
};

pub const CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.ckb.v1.ClientState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Ckb4Ibc
    }

    fn latest_height(&self) -> Height {
        Height::new(1, 1).unwrap()
    }

    fn frozen_height(&self) -> Option<Height> {
        None
    }

    fn expired(&self, _elapsed: core::time::Duration) -> bool {
        false
    }

    fn upgrade(
        &mut self,
        _upgrade_height: Height,
        _upgrade_options: &dyn UpgradeOptions,
        _chain_id: ChainId,
    ) {
    }
}

impl Protobuf<Any> for ClientState {}

impl TryFrom<Any> for ClientState {
    type Error = Ics02Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            CLIENT_STATE_TYPE_URL => {
                let value = serde_json::from_slice::<Self>(&raw.value).unwrap();
                Ok(value)
            }
            _ => Err(Ics02Error::unknown_client_state_type(raw.type_url)),
        }
    }
}

impl From<ClientState> for Any {
    fn from(value: ClientState) -> Self {
        Any {
            type_url: CLIENT_STATE_TYPE_URL.to_string(),
            value: serde_json::to_vec(&value).expect("encoding to `Any` from `CKbClientState`"),
        }
    }
}
