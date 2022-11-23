use crate::core::ics02_client::error::Error as Ics02Error;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Header {}

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
