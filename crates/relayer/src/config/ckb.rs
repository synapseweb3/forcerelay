use std::path::PathBuf;

use ckb_types::H256;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub ckb_rpc: Url,
    pub ckb_indexer_rpc: Url,
    pub lightclient_contract_typeargs: H256,
    pub lightclient_lock_typeargs: H256,
    pub client_type_args: ClientTypeArgs,
    pub minimal_updates_count: u8,
    pub key_name: String,
    pub data_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientTypeArgs {
    // Hash, 32 bytes
    pub type_id: Option<H256>,
    // Number of client cells, plus one info cell
    pub cells_count: u8,
}
