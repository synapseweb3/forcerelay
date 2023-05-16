use ibc_proto::google::protobuf::Any;
use ibc_relayer_types::{
    core::{
        ics03_connection::{
            connection::Counterparty,
            msgs::conn_open_init::{self, MsgConnectionOpenInit},
        },
        ics23_commitment::commitment::CommitmentPrefix,
        ics24_host::identifier::ClientId,
    },
    tx_msg::Msg,
};

use super::contract;
use crate::error::Error;

impl From<&CommitmentPrefix> for contract::MerklePrefixData {
    fn from(value: &CommitmentPrefix) -> Self {
        Self {
            key_prefix: value.clone().into_vec().into(),
        }
    }
}

impl From<Counterparty> for contract::CounterpartyData {
    fn from(value: Counterparty) -> Self {
        let client_id: String = value.client_id().as_str().into();
        Self {
            client_id: value.client_id().as_str().into(),
            connection_id: match value.connection_id() {
                Some(id) => id.as_str().into(),
                None => String::from(""),
            },
            prefix: value.prefix().into(),
        }
    }
}

impl From<MsgConnectionOpenInit> for contract::MsgConnectionOpenInit {
    fn from(value: MsgConnectionOpenInit) -> Self {
        Self {
            client_id: value.client_id.as_str().into(),
            counterparty: value.counterparty.into(),
            delay_period: value.delay_period.as_secs(),
        }
    }
}

impl TryFrom<Any> for contract::MsgConnectionOpenInit {
    type Error = Error;

    fn try_from(value: Any) -> Result<Self, Self::Error> {
        Ok(MsgConnectionOpenInit::from_any(value)
            .map_err(|e| Error::protobuf_decode(conn_open_init::TYPE_URL.into(), e))?
            .into())
    }
}
