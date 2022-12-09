use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub id: ChainId,
    pub websocket_addr: Url,
}
