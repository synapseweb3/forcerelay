use crate::{
    core::{ics02_client::client_type::ClientType, ics23_commitment::commitment::CommitmentRoot},
    timestamp::Timestamp,
};
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

use crate::core::ics02_client::error::Error as Ics02Error;

pub const AXON_CONSENSUS_STATE_TYPE_URL: &str = "/ibc.lightclients.axon.v1.ConsensusState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusState {}

impl crate::core::ics02_client::consensus_state::ConsensusState for ConsensusState {
    fn client_type(&self) -> ClientType {
        todo!()
    }

    fn root(&self) -> &CommitmentRoot {
        todo!()
    }

    fn timestamp(&self) -> Timestamp {
        todo!()
    }
}

impl Protobuf<Any> for ConsensusState {}

impl TryFrom<Any> for ConsensusState {
    type Error = Ics02Error;

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<ConsensusState> for Any {
    fn from(_value: ConsensusState) -> Self {
        todo!()
    }
}
