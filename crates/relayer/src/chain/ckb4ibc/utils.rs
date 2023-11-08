use std::str::FromStr;

use crate::chain::axon::utils::convert_err;
use crate::chain::ckb::prelude::CkbReader;
use crate::chain::SEC_TO_NANO;
use crate::config::ckb4ibc::ChainConfig;
use crate::error::Error;
use crate::event::IbcEventWithHeight;
use ckb_ics_axon::consts::{CHANNEL_ID_PREFIX, CONNECTION_ID_PREFIX};
use ckb_ics_axon::handler::IbcPacket;
use ckb_ics_axon::object::Proofs as CkbProofs;
use ckb_ics_axon::proof::ObjectProof;
use ckb_jsonrpc_types::TransactionView;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_indexer::ScriptSearchMode;
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_sdk::traits::{CellQueryOptions, ValueRangeOption};
use ckb_sdk::NetworkType;
use ckb_types::core::ScriptHashType;
use ckb_types::packed::{Byte32, Bytes, BytesOpt, OutPoint, Script};
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::{h256, H256};
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics04_channel::channel::ChannelEnd;
use ibc_relayer_types::core::ics04_channel::events::{SendPacket, WriteAcknowledgement};
use ibc_relayer_types::core::ics04_channel::packet::{Packet, Sequence};
use ibc_relayer_types::core::ics04_channel::timeout::TimeoutHeight;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use ibc_relayer_types::events::WithBlockDataType;
use ibc_relayer_types::proofs::{ConsensusProof, Proofs};
use ibc_relayer_types::timestamp::Timestamp;
use ibc_relayer_types::Height;
use tiny_keccak::{Hasher, Keccak};

use super::message::MsgToTxConverter;

const SUDT_CODE_HASH_MAINNET: H256 =
    h256!("0x5e7a36a77e68eecc013dfa2fe6a23f3b6c344b04005808694ae6dd45eea4cfd5");
const SUDT_CODE_HASH_TESTNET: H256 =
    h256!("0xc5e5dcf215925f7ef4dfaf5f4b4f105bc321c02776d6e7d52a1db3fcd9d011a4");

pub fn keccak256(slice: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(slice);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    output
}

pub struct EncodedObject {
    pub witness: BytesOpt,
    pub data: Bytes,
}

pub fn get_encoded_object<T: rlp::Encodable>(obj: &T) -> EncodedObject {
    let content = rlp::encode(obj);
    let slice = content.as_ref();
    let hash = keccak256(slice);
    EncodedObject {
        data: hash.as_slice().pack(),
        witness: BytesOpt::new_builder().set(Some(slice.pack())).build(),
    }
}

pub fn convert_proof(ckb_proofs: Proofs) -> Result<CkbProofs, Error> {
    let object_proof_data: Vec<u8> = ckb_proofs.object_proof().clone().into();
    let object_proof = rlp::decode::<ObjectProof>(&object_proof_data)
        .map_err(|_| Error::other_error(String::from("convert object proof error")))?;
    Ok(CkbProofs {
        height: ckb_proofs.height().revision_number().to_le_bytes().to_vec(),
        object_proof,
        client_proof: vec![],
    })
}

pub fn convert_port_id_to_array(port_id: &PortId) -> Result<[u8; 32], Error> {
    let port_id = H256::from_str(port_id.as_str())
        .map_err(|_| Error::ckb_port_id_invalid(port_id.as_str().to_string()))?;
    Ok(port_id.into())
}

pub fn get_script_hash(type_args: &H256) -> Byte32 {
    let script = Script::new_builder()
        .hash_type(ScriptHashType::Type.into())
        .args(type_args.as_bytes().pack())
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .build();
    script.calc_script_hash()
}

pub fn get_channel_number(id: &ChannelId) -> Result<u16, Error> {
    let s = id.as_str();
    let result = s
        .strip_prefix(CHANNEL_ID_PREFIX)
        .ok_or(Error::ckb_chan_id_invalid(s.to_string()))?;
    result
        .parse::<u16>()
        .map_err(|_| Error::ckb_chan_id_invalid(s.to_string()))
}

pub fn get_connection_id_prefix(client_id: &str) -> String {
    // to keep connection_id unique in global
    format!("{}-{CONNECTION_ID_PREFIX}", &client_id[..6])
}

pub fn generate_connection_id(idx: u16, client_id: &str) -> ConnectionId {
    ConnectionId::from_str(&format!("{}{idx}", get_connection_id_prefix(client_id))).unwrap()
}

pub fn generate_channel_id(idx: u16) -> ChannelId {
    ChannelId::from_str(&format!("{CHANNEL_ID_PREFIX}{idx}")).unwrap()
}

pub fn get_connection_index_by_id(id: &ConnectionId) -> Result<u16, Error> {
    let s = id.as_str();
    let result = s
        .split('-')
        .last()
        .ok_or(Error::ckb_conn_id_invalid(s.to_string()))?;
    result
        .parse::<u16>()
        .map_err(|_| Error::ckb_conn_id_invalid(s.to_string()))
}

pub fn get_connection_search_key(
    config: &ChainConfig,
    client_type: Option<ClientType>,
) -> Result<SearchKey, Error> {
    let mut client_id = None;
    if let Some(client_type) = client_type {
        client_id = Some(config.lc_client_id(client_type)?.to_string());
    }
    let script = get_connection_lock_script(config, client_id)?;
    let script_search_mode = if client_type.is_some() {
        Some(ScriptSearchMode::Exact)
    } else {
        Some(ScriptSearchMode::Prefix)
    };
    Ok(SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: None,
        group_by_transaction: None,
        script_search_mode,
    })
}

pub fn get_connection_lock_script(
    config: &ChainConfig,
    client_id: Option<String>,
) -> Result<Script, Error> {
    // fetch specific (concrete client cell) or all (prefix search)
    let mut client_cell_type_args = vec![];
    if let Some(client_id) = client_id {
        let client_type = config.lc_client_type(&client_id)?;
        client_cell_type_args
            .append(&mut config.lc_client_type_hash(client_type)?.as_bytes().to_vec());
    }
    let script = Script::new_builder()
        .code_hash(get_script_hash(&config.connection_type_args))
        .args(client_cell_type_args.pack())
        .hash_type(ScriptHashType::Type.into())
        .build();
    Ok(script)
}

pub fn get_channel_lock_script(converter: &impl MsgToTxConverter, args: Vec<u8>) -> Script {
    Script::new_builder()
        .code_hash(converter.get_channel_code_hash())
        .args(args.pack())
        .hash_type(ScriptHashType::Type.into())
        .build()
}

pub fn get_packet_lock_script(converter: &impl MsgToTxConverter, args: Vec<u8>) -> Script {
    Script::new_builder()
        .code_hash(converter.get_packet_code_hash())
        .args(args.pack())
        .hash_type(ScriptHashType::Type.into())
        .build()
}

pub fn get_prefix_search_key(script: Script) -> SearchKey {
    SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: Some(true),
        group_by_transaction: None,
        script_search_mode: Some(ScriptSearchMode::Prefix),
    }
}

pub fn get_search_key_with_sudt(
    script: Script,
    symbol: &str,
    network: NetworkType,
) -> Result<SearchKey, Error> {
    let sudt_code_hash = match network {
        NetworkType::Mainnet => SUDT_CODE_HASH_MAINNET,
        NetworkType::Testnet => SUDT_CODE_HASH_TESTNET,
        _ => {
            return Err(Error::other_error(format!(
                "unsupported network: {network}"
            )))
        }
    };
    let owner_lockhash =
        H256::from_str(symbol).map_err(|_| Error::other_error("invalid sUDT symbol".to_owned()))?;
    let sudt_script = Script::new_builder()
        .code_hash(sudt_code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(owner_lockhash.as_bytes().to_vec().pack())
        .build();
    let mut query = CellQueryOptions::new_lock(script);
    query.with_data = Some(true);
    query.script_search_mode = Some(ScriptSearchMode::Exact);
    query.secondary_script = Some(sudt_script);
    query.data_len_range = Some(ValueRangeOption::new_exact(16));
    Ok(query.into())
}

pub fn get_dummy_merkle_proof(height: Height) -> Proofs {
    let encoded = rlp::encode(&ObjectProof::default()).to_vec();
    let useless_client_proof = vec![0u8].try_into().unwrap();
    let useless_consensus_proof =
        ConsensusProof::new(vec![0u8].try_into().unwrap(), Height::default()).unwrap();
    Proofs::new(
        encoded.try_into().unwrap(),
        Some(useless_client_proof),
        Some(useless_consensus_proof),
        None,
        height,
    )
    .unwrap()
}

pub fn get_client_outpoint(
    converter: &impl MsgToTxConverter,
    client_id: &str,
) -> Result<OutPoint, Error> {
    converter
        .get_client_outpoint(client_id)
        .ok_or(Error::other_error(format!("not found {client_id}")))
}

pub fn get_client_id_from_channel(
    channel: &ChannelEnd,
    converter: &impl MsgToTxConverter,
) -> Result<([u8; 32], String), Error> {
    let connection_id = channel.connection_hops[0].clone();
    extract_client_id_by_connection_id(&connection_id.to_string(), converter)
}

pub fn extract_client_id_by_connection_id(
    connection_id: &String,
    converter: &impl MsgToTxConverter,
) -> Result<([u8; 32], String), Error> {
    let connection_id = connection_id
        .parse()
        .map_err(|_| Error::other_error(format!("bad connection_id {connection_id}")))?;
    let idx = get_connection_index_by_id(&connection_id)
        .map_err(|_| Error::other_error(format!("bad connection_id {connection_id}")))?;
    let ibc_conn = converter.get_ibc_connections_by_connection_id(&connection_id)?;
    let connection_end = ibc_conn
        .connections
        .get(idx as usize)
        .ok_or(Error::other_error(format!("exceed connection index {idx}")))?;
    let client_id = connection_end.client_id.clone();
    let client_cell_type_args = hex::decode(&client_id)
        .map_err(|_| Error::other_error(format!("client_id {client_id} hex decodeable")))?
        .try_into()
        .map_err(|_| Error::other_error(format!("client_id {client_id} size = 32")))?;
    Ok((client_cell_type_args, client_id))
}

pub fn generate_ibc_packet_event(
    packet: IbcPacket,
    height: u64,
    event_id: &WithBlockDataType,
) -> Result<IbcEventWithHeight, Error> {
    let to_ibc_packet = |v: IbcPacket| -> Result<Packet, Error> {
        let packet = Packet {
            sequence: Sequence::from(v.packet.sequence as u64),
            source_channel: ChannelId::from_str(&v.packet.source_channel_id)
                .map_err(convert_err)?,
            source_port: PortId::from_str(&v.packet.source_port_id).map_err(convert_err)?,
            destination_channel: ChannelId::from_str(&v.packet.destination_channel_id)
                .map_err(convert_err)?,
            destination_port: PortId::from_str(&v.packet.source_port_id).map_err(convert_err)?,
            data: v.packet.data,
            timeout_height: if v.packet.timeout_height == 0 {
                TimeoutHeight::Never
            } else {
                TimeoutHeight::At(Height::from_noncosmos_height(v.packet.timeout_height))
            },
            timeout_timestamp: Timestamp::from_nanoseconds(
                v.packet.timeout_timestamp * SEC_TO_NANO,
            )
            .map_err(convert_err)?,
        };
        Ok(packet)
    };

    let tx_hash = packet.tx_hash.unwrap_or_default().to_fixed_bytes();
    let event = match event_id {
        WithBlockDataType::SendPacket => SendPacket {
            packet: to_ibc_packet(packet)?,
        }
        .into(),
        WithBlockDataType::WriteAck => WriteAcknowledgement {
            ack: packet.ack.clone().unwrap_or_default(),
            packet: to_ibc_packet(packet)?,
        }
        .into(),
        _ => {
            return Err(Error::other_error(
                "unexpected event_id in generation of packet event".to_owned(),
            ));
        }
    };

    Ok(IbcEventWithHeight {
        event,
        tx_hash,
        height: Height::from_noncosmos_height(height),
    })
}

pub async fn fetch_transaction_by_hash(
    rpc_client: &impl CkbReader,
    tx_hash: &H256,
) -> Result<TransactionView, Error> {
    let tx = rpc_client
        .get_transaction(tx_hash)
        .await
        .map_err(|e| Error::query(e.to_string()))?
        .unwrap()
        .transaction
        .unwrap();
    let tx = match tx.inner {
        ckb_jsonrpc_types::Either::Left(tx) => tx,
        ckb_jsonrpc_types::Either::Right(bytes) => {
            serde_json::from_slice(bytes.as_bytes()).unwrap()
        }
    };
    Ok(tx)
}

pub async fn tip_block_number(rpc_client: &impl CkbReader) -> Result<u64, Error> {
    let tip_block_number: u64 = rpc_client
        .get_tip_header()
        .await
        .map_err(|err| Error::other_error(err.to_string()))?
        .inner
        .number
        .into();
    Ok(tip_block_number)
}
