use crate::core::ics02_client::error::Error as Ics02Error;
use ethereum_types::H256;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, TreeHash, Default)]
pub struct Header {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: H256,
    pub state_root: H256,
    pub body_root: H256,
}

impl crate::core::ics02_client::header::Header for Header {
    fn client_type(&self) -> crate::core::ics02_client::client_type::ClientType {
        todo!()
    }

    fn height(&self) -> crate::Height {
        todo!()
    }

    fn timestamp(&self) -> crate::timestamp::Timestamp {
        todo!()
    }
}

impl core::fmt::Debug for Header {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Header").finish()
    }
}

impl Protobuf<Any> for Header {}

impl TryFrom<Any> for Header {
    type Error = Ics02Error;

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<Header> for Any {
    fn from(_header: Header) -> Self {
        todo!()
    }
}
