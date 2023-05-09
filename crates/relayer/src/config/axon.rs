use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::WebSocketClientUrl;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AxonChainConfig {
    pub id: ChainId,
    pub websocket_addr: WebSocketClientUrl,
    pub contract_address: ethers::types::Address,
    pub key_name: String,
    pub store_prefix: String,
}
