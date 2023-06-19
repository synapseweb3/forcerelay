use crate::{
    core::{ics02_client::client_type::ClientType, ics23_commitment::commitment::CommitmentRoot},
    timestamp::Timestamp,
};
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};
use tendermint::Time;

use crate::core::ics02_client::error::Error as Ics02Error;

pub const CKB_CONSENSUS_STATE_TYPE_URL: &str = "/ibc.lightclients.ckb.v1.ConsensusState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusState {
    pub timestamp: Time,
    pub commitment_root: CommitmentRoot,
}

impl crate::core::ics02_client::consensus_state::ConsensusState for ConsensusState {
    fn client_type(&self) -> ClientType {
        ClientType::Ckb4Ibc
    }

    fn root(&self) -> &CommitmentRoot {
        &self.commitment_root
    }

    fn timestamp(&self) -> Timestamp {
        self.timestamp.into()
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
