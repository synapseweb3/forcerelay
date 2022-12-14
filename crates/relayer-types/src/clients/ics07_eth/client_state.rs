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

use crate::clients::ics07_eth::types::Update;
use crate::core::ics02_client::{
    client_state::ClientState as Ics02ClientState, error::Error as Ics02Error,
};
use ibc_proto::protobuf::Protobuf;

const CLIENT_STATE_TYPE_URL: &str = "/eth.client.v1.state";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
    pub lightclient_update: Update,
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Eth
    }

    fn latest_height(&self) -> crate::Height {
        let slot = self.lightclient_update.attested_header.slot;
        crate::Height::new(slot / 32, slot).expect("band attested_header slot")
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
        if any.type_url != CLIENT_STATE_TYPE_URL {
            return Err(Ics02Error::unknown_client_type("ethereum 2.0".to_owned()));
        }
        let client: ClientState = serde_json::from_slice(&any.value)
            .map_err(|e| Ics02Error::unknown_client_state_type(e.to_string()))?;
        Ok(client)
    }
}

impl From<ClientState> for Any {
    fn from(client: ClientState) -> Self {
        let json = serde_json::to_string(&client).expect("jsonify clientstate");
        Any {
            type_url: CLIENT_STATE_TYPE_URL.to_owned(),
            value: json.into_bytes(),
        }
    }
}
