use ethereum_types::H256;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Error as ProtoError;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

use crate::core::ics02_client;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::prelude::*;

pub const FINALITY_HEADER_TYPE_URL: &str = "/eth.finality.v1.update";

#[derive(Clone, PartialEq, Deserialize, Serialize, TreeHash, Default, Debug)]
pub struct Header {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: H256,
    pub state_root: H256,
    pub body_root: H256,
}

impl ics02_client::header::Header for Header {
    fn client_type(&self) -> ics02_client::client_type::ClientType {
        ics02_client::client_type::ClientType::Eth
    }

    fn height(&self) -> crate::Height {
        crate::Height::new(self.slot / 32, self.slot)
            .expect("transform finalized slot to cosmos height")
    }

    fn timestamp(&self) -> crate::timestamp::Timestamp {
        crate::timestamp::Timestamp::none()
    }
}

impl Protobuf<Any> for Header {}

impl TryFrom<Any> for Header {
    type Error = Ics02Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        if any.type_url != FINALITY_HEADER_TYPE_URL {
            return Err(Ics02Error::unknown_header_type("ethereum 2.0".to_owned()));
        }
        let header: Header = serde_json::from_slice(&any.value).map_err(|e| {
            Ics02Error::invalid_raw_header(ProtoError::try_from_protobuf(e.to_string()))
        })?;
        Ok(header)
    }
}

impl From<Header> for Any {
    fn from(header: Header) -> Self {
        let json = serde_json::to_string(&header).expect("jsonify finality update");
        Any {
            type_url: FINALITY_HEADER_TYPE_URL.to_owned(),
            value: json.into_bytes(),
        }
    }
}
