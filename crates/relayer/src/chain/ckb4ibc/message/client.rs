use ckb_ics_axon::message::{Envelope, MsgType};
use ibc_relayer_types::{
    clients::{
        ics07_axon::client_state::AXON_CLIENT_STATE_TYPE_URL,
        ics07_ckb::client_state::CKB_CLIENT_STATE_TYPE_URL,
    },
    core::ics02_client::{
        client_type::ClientType,
        events::{Attributes, CreateClient, UpdateClient},
        msgs::{create_client::MsgCreateClient, update_client::MsgUpdateClient},
    },
    events::IbcEvent,
    Height,
};

use super::{CkbTxInfo, MsgToTxConverter};

use crate::error::Error;

pub fn convert_create_client<C: MsgToTxConverter>(
    msg: MsgCreateClient,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    let (client_type, client_id) = match msg.client_state.type_url.as_str() {
        AXON_CLIENT_STATE_TYPE_URL => (
            ClientType::Axon,
            converter
                .get_config()
                .lc_client_id(ClientType::Axon)
                .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Ckb4Ibc)))?,
        ),
        CKB_CLIENT_STATE_TYPE_URL => (
            ClientType::Ckb4Ibc,
            converter
                .get_config()
                .lc_client_id(ClientType::Ckb4Ibc)
                .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Ckb4Ibc)))?,
        ),
        url => {
            return Err(Error::other_error(format!(
                "unsupport client_state url: {url}"
            )));
        }
    };
    Ok(CkbTxInfo {
        unsigned_tx: None,
        envelope: Envelope {
            msg_type: MsgType::MsgClientCreate,
            content: vec![],
        },
        input_capacity: 0,
        event: Some(IbcEvent::CreateClient(CreateClient(Attributes {
            client_id,
            client_type,
            consensus_height: Height::default(),
        }))),
    })
}

pub fn convert_update_client<C: MsgToTxConverter>(
    msg: MsgUpdateClient,
    _converter: &C,
) -> Result<CkbTxInfo, Error> {
    Ok(CkbTxInfo {
        unsigned_tx: None,
        envelope: Envelope {
            msg_type: MsgType::MsgClientUpdate,
            content: vec![],
        },
        input_capacity: 0,
        event: Some(IbcEvent::UpdateClient(UpdateClient {
            common: Attributes {
                client_id: msg.client_id,
                client_type: ClientType::Ckb4Ibc,
                consensus_height: Height::default(),
            },
            header: None,
        })),
    })
}
