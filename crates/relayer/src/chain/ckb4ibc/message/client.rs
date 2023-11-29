use ckb_ics_axon::{
    handler::IbcConnections,
    message::{Envelope, MsgType},
};
use ckb_types::packed::BytesOpt;
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

use super::{CkbTxInfo, MsgToTxConverter, TxBuilder};

use crate::{
    chain::ckb4ibc::utils::{get_connection_lock_script, get_encoded_object},
    error::Error,
};

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
    // one light client only matches one unique connections cell on CKB, if not exist, create it
    let find_unique_connections = converter.get_ibc_connections(client_id.as_str()).is_ok();
    let unsigned_tx = if !find_unique_connections {
        tracing::info!("connections_cell for {client_type} isn't detected on CKB, create one");
        let empty_ibc_connections = get_encoded_object(&IbcConnections::default());
        let connections_lock_script =
            get_connection_lock_script(converter.get_config(), Some(client_id.to_string()))?;

        let packed_tx = TxBuilder::default()
            .output(connections_lock_script, empty_ibc_connections.data)
            .witness(BytesOpt::default(), empty_ibc_connections.witness)
            .build();
        Some(packed_tx)
    } else {
        None
    };
    Ok(CkbTxInfo {
        unsigned_tx,
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
        commitment_path: Default::default(),
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
        commitment_path: Default::default(),
    })
}
