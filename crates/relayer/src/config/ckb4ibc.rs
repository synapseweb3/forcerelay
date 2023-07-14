use std::str::FromStr;

use super::error::Error;
use ckb_types::H256;
use ibc_relayer_types::core::{
    ics02_client::client_type::ClientType,
    ics24_host::identifier::{ChainId, ClientId},
};
use serde::ser::SerializeMap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use tendermint_rpc::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientItem {
    pub chain_id: ChainId,
    pub client_type_args: H256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub ckb_rpc: Url,
    pub ckb_indexer_rpc: Url,
    pub key_name: String,

    pub connection_type_args: H256,
    pub channel_type_args: H256,
    pub packet_type_args: H256,
    #[serde(serialize_with = "light_client_serialize")]
    pub onchain_light_clients: HashMap<ClientType, LightClientItem>,
}

impl ChainConfig {
    pub fn lc_chain_id(&self, client_id: ClientId) -> Result<ChainId, Error> {
        let chain_id = self
            .onchain_light_clients
            .iter()
            .find_map(|(_, v)| {
                if hex::encode(&v.client_type_args) == client_id.to_string() {
                    Some(v.chain_id.clone())
                } else {
                    None
                }
            })
            .ok_or(Error::invalid(format!("config.toml missing {client_id}")))?;
        Ok(chain_id)
    }

    pub fn lc_client_id(&self, client_type: ClientType) -> Result<ClientId, Error> {
        let (_, item) = self
            .onchain_light_clients
            .iter()
            .find(|(v, _)| **v == client_type)
            .ok_or(Error::invalid(format!("config.toml missing {client_type}")))?;
        let value = hex::encode(&item.client_type_args);
        ClientId::from_str(&value).map_err(|e| Error::invalid(e.to_string()))
    }

    pub fn lc_client_id_bytes(&self, client_type: ClientType) -> Result<[u8; 32], Error> {
        let (_, item) = self
            .onchain_light_clients
            .iter()
            .find(|(v, _)| **v == client_type)
            .ok_or(Error::invalid(format!("config.toml missing {client_type}")))?;
        Ok(item.client_type_args.clone().into())
    }
}

// it's only workable for serializing `onchain_light_clients` filed into JSON string,
// especially for passing config test cases
fn light_client_serialize<S: serde::Serializer>(
    item: &HashMap<ClientType, LightClientItem>,
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut map = s.serialize_map(Some(item.len()))?;
    item.iter()
        .try_for_each(|(k, v)| map.serialize_entry(k.as_str(), v))?;
    map.end()
}
