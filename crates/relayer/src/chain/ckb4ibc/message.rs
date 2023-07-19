mod chan;
mod client;
mod conn;

use std::{cell::Ref, collections::HashMap};

use chan::*;
use conn::*;

use crate::{config::ckb4ibc::ChainConfig, error::Error, keyring::Secp256k1KeyPair};
use ckb_ics_axon::{
    handler::{IbcChannel, IbcConnections},
    message::Envelope,
};
use ckb_types::core::TransactionView;
use ckb_types::packed::{Byte32, CellInput, OutPoint};
use ibc_proto::google::protobuf::Any;
use ibc_relayer_types::{
    core::ics02_client::msgs::{
        create_client::{MsgCreateClient, TYPE_URL as CREATE_CLIENT_TYPE_URL},
        update_client::{MsgUpdateClient, TYPE_URL as UPDATE_CLIENT_TYPE_URL},
    },
    core::ics03_connection::msgs::{
        conn_open_ack::MsgConnectionOpenAck, conn_open_ack::TYPE_URL as CONN_OPEN_ACK_TYPE_URL,
        conn_open_confirm::MsgConnectionOpenConfirm,
        conn_open_confirm::TYPE_URL as CONN_OPEN_CONFIRM_TYPE_URL,
        conn_open_init::MsgConnectionOpenInit, conn_open_init::TYPE_URL as CONN_OPEN_INIT_TYPE_URL,
        conn_open_try::MsgConnectionOpenTry, conn_open_try::TYPE_URL as CONN_OPEN_TRY_TYPE_URL,
    },
    core::{
        ics02_client::client_type::ClientType,
        ics03_connection::connection::IdentifiedConnectionEnd,
        ics04_channel::{
            msgs::{
                acknowledgement::MsgAcknowledgement,
                acknowledgement::TYPE_URL as ACK_TYPE_URL,
                chan_close_init::MsgChannelCloseInit,
                chan_close_init::TYPE_URL as CHAN_CLOSE_INIT_TYPE_URL,
                chan_open_ack::MsgChannelOpenAck,
                chan_open_ack::TYPE_URL as CHAN_OPEN_ACK_TYPE_URL,
                chan_open_confirm::MsgChannelOpenConfirm,
                chan_open_confirm::TYPE_URL as CHAN_OPEN_CONFIRM_TYPE_URL,
                chan_open_init::MsgChannelOpenInit,
                chan_open_init::TYPE_URL as CHAN_OPEN_INIT_TYPE_URL,
                chan_open_try::MsgChannelOpenTry,
                chan_open_try::TYPE_URL as CHAN_OPEN_TRY_TYPE_URL,
                recv_packet::{MsgRecvPacket, TYPE_URL as RECV_PACKET_TYPE_URL},
            },
            packet::Sequence,
        },
        ics24_host::identifier::{ChannelId, ConnectionId, PortId},
    },
    events::IbcEvent,
    tx_msg::Msg,
};

use self::client::{convert_create_client, convert_update_client};

use super::utils::{generate_connection_id, get_script_hash};

macro_rules! convert {
    ($msg:ident, $conval:ident, $msgty:ty, $conv:ident) => {{
        let msg = <$msgty>::from_any($msg.clone())
            .map_err(|e| Error::protobuf_decode($msg.type_url.clone(), e))?;
        $conv(msg, $conval)
    }};
}

pub trait MsgToTxConverter {
    fn get_key(&self) -> &Secp256k1KeyPair;

    fn get_ibc_connections(&self, client_id: &str) -> Result<IbcConnections, Error>;

    fn get_ibc_connections_by_connection_id(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<IbcConnections, Error>;

    fn get_ibc_connections_by_port_id(
        &self,
        channel_id: &ChannelId,
    ) -> Result<IbcConnections, Error>;

    fn get_ibc_connections_input(&self, client_id: &str) -> Result<CellInput, Error>;

    fn get_ibc_channel(&self, id: &ChannelId) -> IbcChannel;

    fn get_ibc_channel_input(&self, channel_id: &ChannelId, port_id: &PortId) -> CellInput;

    fn get_client_outpoint(&self, client_id: &str) -> Option<&OutPoint>;

    fn get_conn_contract_outpoint(&self) -> OutPoint;

    fn get_chan_contract_outpoint(&self) -> OutPoint;

    fn get_packet_contract_outpoint(&self) -> OutPoint;

    fn get_channel_code_hash(&self) -> Byte32;

    fn get_packet_code_hash(&self) -> Byte32;

    fn get_connection_code_hash(&self) -> Byte32;

    fn get_packet_cell_input(&self, chan: ChannelId, port: PortId, seq: Sequence) -> CellInput;

    fn get_packet_owner(&self) -> [u8; 32];

    fn get_config(&self) -> &ChainConfig;
}

pub struct Converter<'a> {
    pub channel_input_data: Ref<'a, HashMap<(ChannelId, PortId), CellInput>>,
    pub channel_cache: Ref<'a, HashMap<ChannelId, IbcChannel>>,
    #[allow(clippy::type_complexity)]
    pub connection_cache:
        Ref<'a, HashMap<ClientType, (IbcConnections, CellInput, Vec<IdentifiedConnectionEnd>)>>,
    pub packet_input_data: Ref<'a, HashMap<(ChannelId, PortId, Sequence), CellInput>>,
    pub config: &'a ChainConfig,
    pub client_outpoints: Ref<'a, HashMap<ClientType, OutPoint>>,
    pub chan_contract_outpoint: &'a OutPoint,
    pub packet_contract_outpoint: &'a OutPoint,
    pub conn_contract_outpoint: &'a OutPoint,
    pub packet_owner: [u8; 32],
}

impl<'a> MsgToTxConverter for Converter<'a> {
    fn get_key(&self) -> &Secp256k1KeyPair {
        todo!()
    }

    fn get_ibc_connections(&self, client_id: &str) -> Result<IbcConnections, Error> {
        let client_type = self.config.lc_client_type(client_id)?;
        if let Some((connection, _, _)) = self.connection_cache.get(&client_type) {
            Ok(connection.clone())
        } else {
            Err(Error::query(format!(
                "client_type {client_type} isn't in cache"
            )))
        }
    }

    fn get_ibc_connections_by_connection_id(
        &self,
        connection_id: &ConnectionId,
    ) -> Result<IbcConnections, Error> {
        let ibc_connections = self.connection_cache.iter().find(|(_, (v, _, _))| {
            v.connections
                .iter()
                .enumerate()
                .any(|(idx, c)| connection_id == &generate_connection_id(idx as u16, &c.client_id))
        });
        if let Some((_, (value, _, _))) = ibc_connections {
            Ok(value.clone())
        } else {
            Err(Error::query(format!(
                "connection {connection_id} not found in cache"
            )))
        }
    }

    fn get_ibc_connections_by_port_id(
        &self,
        channel_id: &ChannelId,
    ) -> Result<IbcConnections, Error> {
        let channel = self
            .channel_cache
            .get(channel_id)
            .ok_or_else(|| Error::query(format!("channel {channel_id} not found in cache")))?;
        // FIXME: should modify ibc contract
        let connection_id = channel.connection_hops[0].parse().unwrap();
        self.get_ibc_connections_by_connection_id(&connection_id)
    }

    fn get_ibc_connections_input(&self, client_id: &str) -> Result<CellInput, Error> {
        let client_type = self.config.lc_client_type(client_id)?;
        if let Some((_, cell_input, _)) = self.connection_cache.get(&client_type) {
            Ok(cell_input.clone())
        } else {
            Err(Error::query(format!(
                "client_type {client_type} isn't in cache"
            )))
        }
    }

    fn get_ibc_channel(&self, channel_id: &ChannelId) -> IbcChannel {
        self.channel_cache.get(channel_id).unwrap().clone()
    }

    fn get_ibc_channel_input(&self, channel_id: &ChannelId, port_id: &PortId) -> CellInput {
        self.channel_input_data
            .get(&(channel_id.clone(), port_id.clone()))
            .unwrap()
            .clone()
    }

    fn get_client_outpoint(&self, client_id: &str) -> Option<&OutPoint> {
        let Some(client_type) = self.config.lc_client_type(client_id).ok() else {
            return None;
        };
        self.client_outpoints.get(&client_type)
    }

    fn get_conn_contract_outpoint(&self) -> OutPoint {
        self.conn_contract_outpoint.clone()
    }

    fn get_chan_contract_outpoint(&self) -> OutPoint {
        self.chan_contract_outpoint.clone()
    }

    fn get_packet_contract_outpoint(&self) -> OutPoint {
        self.packet_contract_outpoint.clone()
    }

    fn get_channel_code_hash(&self) -> Byte32 {
        get_script_hash(&self.config.channel_type_args)
    }

    fn get_packet_code_hash(&self) -> Byte32 {
        get_script_hash(&self.config.packet_type_args)
    }

    fn get_connection_code_hash(&self) -> Byte32 {
        get_script_hash(&self.config.connection_type_args)
    }

    fn get_packet_cell_input(
        &self,
        channel_id: ChannelId,
        port_id: PortId,
        sequence: Sequence,
    ) -> CellInput {
        self.packet_input_data
            .get(&(channel_id, port_id, sequence))
            .unwrap()
            .clone()
    }

    fn get_packet_owner(&self) -> [u8; 32] {
        self.packet_owner
    }

    fn get_config(&self) -> &ChainConfig {
        self.config
    }
}

pub struct CkbTxInfo {
    pub unsigned_tx: Option<TransactionView>,
    pub envelope: Envelope,
    pub input_capacity: u64,
    pub event: Option<IbcEvent>,
}

// Return a transaction which needs to be added relayer's input in it and to be signed.
pub fn convert_msg_to_ckb_tx<C: MsgToTxConverter>(
    msg: Any,
    converter: &C,
) -> Result<CkbTxInfo, Error> {
    match msg.type_url.as_str() {
        // client
        CREATE_CLIENT_TYPE_URL => convert!(msg, converter, MsgCreateClient, convert_create_client),
        UPDATE_CLIENT_TYPE_URL => convert!(msg, converter, MsgUpdateClient, convert_update_client),
        // connection
        CONN_OPEN_INIT_TYPE_URL => convert!(
            msg,
            converter,
            MsgConnectionOpenInit,
            convert_conn_open_init_to_tx
        ),
        CONN_OPEN_TRY_TYPE_URL => convert!(
            msg,
            converter,
            MsgConnectionOpenTry,
            convert_conn_open_try_to_tx
        ),
        CONN_OPEN_ACK_TYPE_URL => convert!(
            msg,
            converter,
            MsgConnectionOpenAck,
            convert_conn_open_ack_to_tx
        ),
        CONN_OPEN_CONFIRM_TYPE_URL => convert!(
            msg,
            converter,
            MsgConnectionOpenConfirm,
            convert_conn_open_confirm_to_tx
        ),
        // chanel
        CHAN_OPEN_INIT_TYPE_URL => convert!(
            msg,
            converter,
            MsgChannelOpenInit,
            convert_chan_open_init_to_tx
        ),
        CHAN_OPEN_TRY_TYPE_URL => convert!(
            msg,
            converter,
            MsgChannelOpenTry,
            convert_chan_open_try_to_tx
        ),
        CHAN_OPEN_ACK_TYPE_URL => convert!(
            msg,
            converter,
            MsgChannelOpenAck,
            convert_chan_open_ack_to_tx
        ),
        CHAN_OPEN_CONFIRM_TYPE_URL => convert!(
            msg,
            converter,
            MsgChannelOpenConfirm,
            convert_chan_open_confirm_to_tx
        ),
        CHAN_CLOSE_INIT_TYPE_URL => convert!(
            msg,
            converter,
            MsgChannelCloseInit,
            convert_chan_close_init_to_tx
        ),
        // packet
        RECV_PACKET_TYPE_URL => convert!(msg, converter, MsgRecvPacket, convert_recv_packet_to_tx),
        ACK_TYPE_URL => convert!(msg, converter, MsgAcknowledgement, convert_ack_packet_to_tx),
        _ => todo!(),
    }
}
