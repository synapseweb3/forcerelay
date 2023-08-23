use std::str::FromStr;

use crate::config::ckb4ibc::ChainConfig;
use crate::error::Error;
use ckb_ics_axon::consts::{
    CHANNEL_CELL_CAPACITY, CHANNEL_ID_PREFIX, CONNECTION_CELL_CAPACITY, CONNECTION_ID_PREFIX,
    PACKET_CELL_CAPACITY,
};
use ckb_ics_axon::object::Proofs as CkbProofs;
use ckb_ics_axon::proof::ObjectProof;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey, SearchKeyFilter};
use ckb_sdk::NetworkType;
use ckb_types::core::{Capacity, ScriptHashType};
use ckb_types::packed::{Byte32, Bytes, BytesOpt, OutPoint, Script};
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::{h256, H256};
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics04_channel::channel::ChannelEnd;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use ibc_relayer_types::proofs::{ConsensusProof, Proofs};
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

pub fn get_encoded_object<T: rlp::Encodable>(obj: T) -> EncodedObject {
    let content = rlp::encode(&obj);
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
    Ok(SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: None,
        group_by_transaction: None,
        script_search_mode: None,
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

pub fn get_search_key(script: Script) -> SearchKey {
    SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: Some(true),
        group_by_transaction: None,
        script_search_mode: None,
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
    let filter = SearchKeyFilter {
        script: Some(sudt_script.into()),
        ..Default::default()
    };
    Ok(SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: Some(filter),
        with_data: Some(true),
        group_by_transaction: None,
        script_search_mode: None,
    })
}

pub fn get_connection_capacity() -> Capacity {
    Capacity::bytes(CONNECTION_CELL_CAPACITY as usize).unwrap()
}

pub fn get_channel_capacity() -> Capacity {
    Capacity::bytes(CHANNEL_CELL_CAPACITY as usize).unwrap()
}

pub fn get_packet_capacity() -> Capacity {
    Capacity::bytes(PACKET_CELL_CAPACITY as usize).unwrap()
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
        .cloned()
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
