use ckb_ics_axon::message::{Envelope, MsgType};
use ibc_relayer_types::{
    core::ics02_client::{
        client_type::ClientType,
        events::{Attributes, UpdateClient},
        msgs::update_client::MsgUpdateClient,
    },
    events::IbcEvent,
    Height,
};

use super::{CkbTxInfo, MsgToTxConverter};

use crate::error::Error;

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
                consensus_height: Height::new(1, u64::MAX).unwrap(),
            },
            header: None,
        })),
    })
}
