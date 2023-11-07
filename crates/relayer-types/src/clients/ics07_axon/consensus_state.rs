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
pub struct AxonConsensusState {
    pub root: CommitmentRoot,
    pub timestamp: Timestamp,
}

impl crate::core::ics02_client::consensus_state::ConsensusState for AxonConsensusState {
    fn client_type(&self) -> ClientType {
        ClientType::Axon
    }

    fn root(&self) -> &CommitmentRoot {
        &self.root
    }

    fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

impl Protobuf<Any> for AxonConsensusState {}

impl TryFrom<Any> for AxonConsensusState {
    type Error = Ics02Error;

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<AxonConsensusState> for Any {
    fn from(_value: AxonConsensusState) -> Self {
        todo!()
    }
}
