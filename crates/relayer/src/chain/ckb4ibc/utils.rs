use std::str::FromStr;

use crate::config::ckb4ibc::ChainConfig;
use crate::error::Error;
use ckb_ics_axon::consts::{
    CHANNEL_CELL_CAPACITY, CHANNEL_ID_PREFIX, CONNECTION_CELL_CAPACITY, CONNECTION_ID_PREFIX,
    PACKET_CELL_CAPACITY,
};
use ckb_ics_axon::object::Proofs as CkbProofs;
use ckb_ics_axon::proof::ObjectProof;
use ckb_ics_axon::ConnectionArgs;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_types::core::{Capacity, ScriptHashType};
use ckb_types::packed::{Byte32, Bytes, BytesOpt, Script};
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::H256;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use ibc_relayer_types::proofs::{ConsensusProof, Proofs};
use ibc_relayer_types::Height;
use tiny_keccak::{Hasher, Keccak};

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

// pub fn get_channel_id(idx: u16) -> ChannelId {
//     ChannelId::from_str(&format!("{CHANNEL_ID_PREFIX}{idx}")).unwrap()
// }

pub fn get_channel_idx(id: &ChannelId) -> Result<u16, Error> {
    let s = id.as_str();
    let result = s
        .strip_prefix(CHANNEL_ID_PREFIX)
        .ok_or(Error::ckb_chan_id_invalid(s.to_string()))?;
    result
        .parse::<u16>()
        .map_err(|_| Error::ckb_chan_id_invalid(s.to_string()))
}

pub fn generate_connection_id(idx: u16) -> ConnectionId {
    ConnectionId::from_str(&format!("{CONNECTION_ID_PREFIX}{idx}")).unwrap()
}

pub fn get_connection_index_by_id(id: &ConnectionId) -> Result<u16, Error> {
    let s = id.as_str();
    let result = s
        .strip_prefix(CONNECTION_ID_PREFIX)
        .ok_or(Error::ckb_conn_id_invalid(s.to_string()))?;
    result
        .parse::<u16>()
        .map_err(|_| Error::ckb_conn_id_invalid(s.to_string()))
}

pub fn get_connection_search_key(config: &ChainConfig) -> SearchKey {
    let script = get_connection_lock_script(config);
    SearchKey {
        script: script.into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: None,
        group_by_transaction: None,
    }
}

pub fn get_connection_lock_script(config: &ChainConfig) -> Script {
    Script::new_builder()
        .code_hash(get_script_hash(&config.connection_type_args))
        .args(
            ConnectionArgs {
                client_id: config.client_type_args.clone().into(),
            }
            .client_id
            .as_slice()
            .pack(),
        )
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
    }
}

#[inline]
pub fn get_connection_capacity() -> Capacity {
    Capacity::bytes(CONNECTION_CELL_CAPACITY as usize).unwrap()
}

#[inline]
pub fn get_channel_capacity() -> Capacity {
    Capacity::bytes(CHANNEL_CELL_CAPACITY as usize).unwrap()
}

#[inline]
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
