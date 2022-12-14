use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint::Hash;
use tendermint_rpc::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub websocket_addr: Url,
    pub lightclient_rpc: Url,
    pub initial_checkpoint: Hash,
    pub key_name: String,
}
