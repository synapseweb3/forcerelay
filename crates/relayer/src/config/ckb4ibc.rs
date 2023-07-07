use std::str::FromStr;

use ckb_types::H256;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub counter_chain: ChainId,
    pub ckb_rpc: Url,
    pub ckb_indexer_rpc: Url,
    pub key_name: String,

    pub client_type_args: H256,
    pub connection_type_args: H256,
    pub channel_type_args: H256,
    pub packet_type_args: H256,
}

impl ChainConfig {
    pub fn client_id(&self) -> ClientId {
        let value = format!("{:x}", self.client_type_args);
        ClientId::from_str(&value).unwrap()
    }

    pub fn client_id_bytes(&self) -> [u8; 32] {
        self.client_type_args.clone().into()
    }
}
