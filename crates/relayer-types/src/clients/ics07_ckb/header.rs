use core::fmt::{Display, Error as FmtError, Formatter};

use alloc::string::ToString;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};

use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::timestamp::Timestamp;
use crate::Height;

pub const CKB_HEADER_TYPE_URL: &str = "/ibc.lightclients.ckb.v1.Header";

// FIXME: useless Ckb header which cannot be ignored by Hermes runtime
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Header {}

impl core::fmt::Debug for Header {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(f, " Header {{...}}")
    }
}

impl Display for Header {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(f, "Header {{}}")
    }
}

impl crate::core::ics02_client::header::Header for Header {
    fn client_type(&self) -> ClientType {
        ClientType::Ckb
    }

    fn height(&self) -> Height {
        Height::default()
    }

    fn timestamp(&self) -> Timestamp {
        Timestamp::none()
    }
}

impl Protobuf<Any> for Header {}

impl TryFrom<Any> for Header {
    type Error = Ics02Error;

    fn try_from(raw: Any) -> Result<Self, Ics02Error> {
        let value = match raw.type_url.as_str() {
            CKB_HEADER_TYPE_URL => Ok(serde_json::from_slice::<Header>(&raw.value).unwrap()),
            _ => Err(Ics02Error::unknown_header_type(raw.type_url)),
        }?;
        Ok(value)
    }
}

impl From<Header> for Any {
    fn from(header: Header) -> Self {
        Any {
            type_url: CKB_HEADER_TYPE_URL.to_string(),
            value: serde_json::to_vec(&header).unwrap(),
        }
    }
}
