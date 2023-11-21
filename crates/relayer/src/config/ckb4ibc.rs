use ckb_ics_axon::ConnectionArgs;
use ckb_types::{
    core::ScriptHashType,
    packed::Script,
    prelude::{Builder, Entity, Pack, Unpack},
    H160, H256,
};
use ibc_relayer_types::core::{
    ics02_client::client_type::ClientType,
    ics24_host::identifier::{ChainId, ClientId},
};
use serde::ser::SerializeMap;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use tendermint_rpc::Url;

use crate::error::Error;

use super::filter::PacketFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientItem {
    pub chain_id: ChainId,
    pub client_cell_type_args: H256,
    pub ibc_handler_address: H160,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub ckb_rpc: Url,
    pub ckb_indexer_rpc: Url,
    pub key_name: String,
    pub store_prefix: String,

    pub client_code_hash: H256,
    pub connection_type_args: H256,
    pub channel_type_args: H256,
    pub packet_type_args: H256,

    #[serde(default)]
    pub packet_filter: PacketFilter,

    #[serde(serialize_with = "light_client_serialize")]
    pub onchain_light_clients: HashMap<ClientType, LightClientItem>,
}

impl ChainConfig {
    pub fn lc_chain_id_by_client_id(&self, client_id: &str) -> Result<ChainId, Error> {
        let chain_id = self
            .onchain_light_clients
            .iter()
            .find_map(|(_, v)| {
                let metadata_type_hash =
                    calc_type_hash(&self.client_code_hash, &v.client_cell_type_args);
                let v_client_id = ConnectionArgs {
                    metadata_type_id: metadata_type_hash.0,
                    ibc_handler_address: v.ibc_handler_address.0,
                }
                .client_id();
                if client_id == v_client_id {
                    Some(v.chain_id.clone())
                } else {
                    None
                }
            })
            .ok_or(Error::other_error(format!(
                "config.toml missing client_id {client_id}"
            )))?;
        Ok(chain_id)
    }

    pub fn lc_client_type(&self, client_id: &str) -> Result<ClientType, Error> {
        let client_type = self
            .onchain_light_clients
            .iter()
            .find_map(|(k, v)| {
                let metadata_type_hash =
                    calc_type_hash(&self.client_code_hash, &v.client_cell_type_args);
                let v_client_id = ConnectionArgs {
                    metadata_type_id: metadata_type_hash.0,
                    ibc_handler_address: v.ibc_handler_address.0,
                }
                .client_id();
                if client_id == v_client_id {
                    Some(*k)
                } else {
                    None
                }
            })
            .ok_or(Error::other_error(format!(
                "config.toml missing client_id {client_id}"
            )))?;
        Ok(client_type)
    }

    pub fn lc_connection_args_by_id(&self, client_id: &str) -> Result<ConnectionArgs, Error> {
        self.onchain_light_clients
            .iter()
            .find_map(|(_, v)| {
                let metadata_type_hash =
                    calc_type_hash(&self.client_code_hash, &v.client_cell_type_args);
                let args = ConnectionArgs {
                    metadata_type_id: metadata_type_hash.0,
                    ibc_handler_address: v.ibc_handler_address.0,
                };
                if client_id == args.client_id() {
                    Some(args)
                } else {
                    None
                }
            })
            .ok_or(Error::other_error(format!(
                "config.toml missing client_id {client_id}"
            )))
    }

    pub fn lc_client_id(&self, client_type: ClientType) -> Result<ClientId, Error> {
        let lc_item = self
            .onchain_light_clients
            .get(&client_type)
            .ok_or_else(|| {
                Error::other_error(format!("config.toml missing client_type {client_type}"))
            })?;
        let args = ConnectionArgs {
            metadata_type_id: calc_type_hash(
                &self.client_code_hash,
                &lc_item.client_cell_type_args,
            )
            .0,
            ibc_handler_address: lc_item.ibc_handler_address.0,
        };
        Ok(args.client_id().parse().unwrap())
    }

    pub fn lc_connection_args(&self, client_type: ClientType) -> Result<ConnectionArgs, Error> {
        let lc_item = self
            .onchain_light_clients
            .get(&client_type)
            .ok_or_else(|| {
                Error::other_error(format!("config.toml missing client_type {client_type}"))
            })?;
        let args = ConnectionArgs {
            metadata_type_id: calc_type_hash(
                &self.client_code_hash,
                &lc_item.client_cell_type_args,
            )
            .0,
            ibc_handler_address: lc_item.ibc_handler_address.0,
        };
        Ok(args)
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

fn calc_type_hash(client_code_hash: &H256, client_type_args: &H256) -> H256 {
    let client_type_hash = Script::new_builder()
        .code_hash(client_code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(client_type_args.as_bytes().pack())
        .build()
        .calc_script_hash();
    client_type_hash.unpack()
}
