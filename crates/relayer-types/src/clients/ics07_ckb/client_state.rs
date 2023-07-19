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

pub const CKB_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.ckb.v1.ClientState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CkbClientState {
    pub chain_id: ChainId,
    pub latest_height: Height,
}

impl Ics02ClientState for CkbClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Ckb4Ibc
    }

    fn latest_height(&self) -> Height {
        self.latest_height
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

impl Protobuf<Any> for CkbClientState {}

impl TryFrom<Any> for CkbClientState {
    type Error = Ics02Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            CKB_CLIENT_STATE_TYPE_URL => {
                let value = serde_json::from_slice::<Self>(&raw.value).unwrap();
                Ok(value)
            }
            _ => Err(Ics02Error::unknown_client_state_type(raw.type_url)),
        }
    }
}

impl From<CkbClientState> for Any {
    fn from(value: CkbClientState) -> Self {
        Any {
            type_url: CKB_CLIENT_STATE_TYPE_URL.to_string(),
            value: serde_json::to_vec(&value).expect("encoding to `Any` from `CKbClientState`"),
        }
    }
}
