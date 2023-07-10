use crate::core::ics02_client::{self, error::Error as Ics02Error};
use crate::prelude::*;
use crate::timestamp::Timestamp;
use crate::Height;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::{Error as ProtoError, Protobuf};
use serde::{Deserialize, Serialize};

pub const AXON_HEADER_TYPE_URL: &str = "/axon.v1.header";

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, Debug)]
pub struct Header {}

impl ics02_client::header::Header for Header {
    fn client_type(&self) -> ics02_client::client_type::ClientType {
        ics02_client::client_type::ClientType::Axon
    }

    fn height(&self) -> Height {
        Height::max()
    }

    fn timestamp(&self) -> Timestamp {
        Timestamp::none()
    }
}

impl Protobuf<Any> for Header {}

impl TryFrom<Any> for Header {
    type Error = Ics02Error;

    fn try_from(any: Any) -> Result<Self, Self::Error> {
        if any.type_url != AXON_HEADER_TYPE_URL {
            return Err(Ics02Error::unknown_header_type("axon".to_owned()));
        }
        let header: Header = serde_json::from_slice(&any.value).map_err(|e| {
            Ics02Error::invalid_raw_header(ProtoError::try_from_protobuf(e.to_string()))
        })?;
        Ok(header)
    }
}

impl From<Header> for Any {
    fn from(header: Header) -> Self {
        let json = serde_json::to_string(&header).expect("jsonify axon header");
        Any {
            type_url: AXON_HEADER_TYPE_URL.to_owned(),
            value: json.into_bytes(),
        }
    }
}
