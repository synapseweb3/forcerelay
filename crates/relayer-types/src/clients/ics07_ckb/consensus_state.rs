use crate::{
    core::{ics02_client::client_type::ClientType, ics23_commitment::commitment::CommitmentRoot},
    timestamp::Timestamp,
};
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

use crate::core::ics02_client::error::Error as Ics02Error;

pub const CKB_CONSENSUS_STATE_TYPE_URL: &str = "/ibc.lightclients.ckb.v1.ConsensusState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CkbConsensusState {}

impl crate::core::ics02_client::consensus_state::ConsensusState for CkbConsensusState {
    fn client_type(&self) -> ClientType {
        ClientType::Ckb4Ibc
    }

    fn root(&self) -> &CommitmentRoot {
        todo!()
    }

    fn timestamp(&self) -> Timestamp {
        Timestamp::none()
    }
}

impl Protobuf<Any> for CkbConsensusState {}

impl TryFrom<Any> for CkbConsensusState {
    type Error = Ics02Error;

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<CkbConsensusState> for Any {
    fn from(_value: CkbConsensusState) -> Self {
        todo!()
    }
}
