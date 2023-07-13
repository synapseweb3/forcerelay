use ckb_ics_axon::message::{Envelope, MsgType};
use ibc_relayer_types::{
    clients::{
        ics07_axon::client_state::{AxonClientState, AXON_CLIENT_STATE_TYPE_URL},
        ics07_ckb::client_state::{CkbClientState, CKB_CLIENT_STATE_TYPE_URL},
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
    _converter: &C,
) -> Result<CkbTxInfo, Error> {
    let client_id = match msg.client_state.type_url.as_str() {
        AXON_CLIENT_STATE_TYPE_URL => {
            AxonClientState::try_from(msg.client_state)
                .unwrap()
                .default_client_id
        }
        CKB_CLIENT_STATE_TYPE_URL => {
            CkbClientState::try_from(msg.client_state)
                .unwrap()
                .default_client_id
        }
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
            client_type: ClientType::Ckb4Ibc,
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
