use ckb_types::H256;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub ckb_rpc: Url,
    pub ckb_indexer_rpc: Url,
    pub key_name: String,

    pub client_type_args: H256,
    pub connection_type_args: H256,
    pub channel_type_args: H256,
    pub packet_type_args: H256,

    pub client_id: [u8; 32],
}
