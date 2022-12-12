use core::fmt::{Display, Error as FmtError, Formatter};

use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};

use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::timestamp::Timestamp;
use crate::Height;

/// Tendermint consensus header
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
        todo!()
    }

    fn timestamp(&self) -> Timestamp {
        todo!()
    }
}

impl Protobuf<Any> for Header {}

impl TryFrom<Any> for Header {
    type Error = Ics02Error;

    fn try_from(_raw: Any) -> Result<Self, Ics02Error> {
        todo!()
    }
}

impl From<Header> for Any {
    fn from(_header: Header) -> Self {
        todo!()
    }
}
