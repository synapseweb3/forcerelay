use crate::core::ics02_client::{self, error::Error as Ics02Error};
// use axon_tools::types::AxonHeader;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, Default, Debug)]
pub struct Header {}

impl ics02_client::header::Header for Header {
    fn client_type(&self) -> ics02_client::client_type::ClientType {
        ics02_client::client_type::ClientType::Axon
    }

    fn height(&self) -> crate::Height {
        todo!()
    }

    fn timestamp(&self) -> crate::timestamp::Timestamp {
        crate::timestamp::Timestamp::none()
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
    fn from(_value: Header) -> Self {
        todo!()
    }
}
