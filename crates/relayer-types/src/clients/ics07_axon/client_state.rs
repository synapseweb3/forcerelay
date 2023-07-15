use crate::{
    core::{
        ics02_client::{
            client_state::ClientState as Ics02ClientState, client_state::UpgradeOptions,
            client_type::ClientType, error::Error as Ics02Error,
        },
        ics24_host::identifier::ChainId,
    },
    prelude::*,
    Height,
};
use core::convert::TryFrom;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

pub const AXON_CLIENT_STATE_TYPE_URL: &str = "/axon.client.v1.state";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxonClientState {
    pub chain_id: ChainId,
    pub latest_height: Height,
}

impl Ics02ClientState for AxonClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Axon
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
        todo!()
    }
}

impl Protobuf<Any> for AxonClientState {}

impl TryFrom<Any> for AxonClientState {
    type Error = Ics02Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        if any.type_url != AXON_CLIENT_STATE_TYPE_URL {
            return Err(Ics02Error::unknown_client_type("axon".to_owned()));
        }
        let client: AxonClientState = serde_json::from_slice(&any.value)
            .map_err(|e| Ics02Error::unknown_client_state_type(e.to_string()))?;
        Ok(client)
    }
}

impl From<AxonClientState> for Any {
    fn from(client: AxonClientState) -> Self {
        let json = serde_json::to_string(&client).expect("jsonify axon client");
        Any {
            type_url: AXON_CLIENT_STATE_TYPE_URL.to_owned(),
            value: json.into_bytes(),
        }
    }
}
