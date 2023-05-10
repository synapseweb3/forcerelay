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
use axon_tools::types::{AxonBlock, Validator};
use core::convert::TryFrom;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

pub const AXON_CLIENT_STATE_TYPE_URL: &str = "/axon.client.v1.state";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
    pub axon_block: AxonBlock,
    pub validator_list: Vec<Validator>,
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Axon
    }

    fn latest_height(&self) -> Height {
        Height::new(u64::MAX, self.axon_block.header.number).expect("bad axon block")
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

impl Protobuf<Any> for ClientState {}

impl TryFrom<Any> for ClientState {
    type Error = Ics02Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        if any.type_url != AXON_CLIENT_STATE_TYPE_URL {
            return Err(Ics02Error::unknown_client_type("axon".to_owned()));
        }
        let client: ClientState = serde_json::from_slice(&any.value)
            .map_err(|e| Ics02Error::unknown_client_state_type(e.to_string()))?;
        Ok(client)
    }
}

impl From<ClientState> for Any {
    fn from(client: ClientState) -> Self {
        let json = serde_json::to_string(&client).expect("jsonify axon client");
        Any {
            type_url: AXON_CLIENT_STATE_TYPE_URL.to_owned(),
            value: json.into_bytes(),
        }
    }
}
