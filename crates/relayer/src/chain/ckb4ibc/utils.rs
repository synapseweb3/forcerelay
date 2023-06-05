use crate::error::Error;
use ckb_ics_axon::consts::{CHANNEL_ID_PREFIX, CONNECTION_ID_PREFIX};
use ckb_ics_axon::object::Proofs as CkbProofs;
use ckb_ics_axon::proof::ObjectProof;
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_types::core::ScriptHashType;
use ckb_types::packed::{Byte32, Bytes, BytesOpt, Script};
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::H256;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ConnectionId};
use ibc_relayer_types::proofs::Proofs;
use tiny_keccak::{Hasher, Keccak};

fn keccak256(slice: &[u8]) -> [u8; 32] {
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
        data: hash.to_vec().pack(),
        witness: BytesOpt::new_builder()
            .set(Some(slice.to_vec().pack()))
            .build(),
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

pub fn get_script_hash(type_args: H256) -> Byte32 {
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

// pub fn get_connection_id(idx: u16) -> ConnectionId {
//     ConnectionId::from_str(&format!("{CONNECTION_ID_PREFIX}{idx}")).unwrap()
// }

pub fn get_connection_idx(id: &ConnectionId) -> Result<u16, Error> {
    let s = id.as_str();
    let result = s
        .strip_prefix(CONNECTION_ID_PREFIX)
        .ok_or(Error::ckb_conn_id_invalid(s.to_string()))?;
    result
        .parse::<u16>()
        .map_err(|_| Error::ckb_conn_id_invalid(s.to_string()))
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
