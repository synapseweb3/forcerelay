use ethers::types::H160;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;
use tendermint_rpc::WebSocketClientUrl;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AxonChainConfig {
    pub id: ChainId,
    pub websocket_addr: WebSocketClientUrl,
    pub rpc_addr: Url,
    pub contract_address: ethers::types::Address,
    pub key_name: String,
    pub store_prefix: String,
    pub ckb_light_client_contract_address: H160,
    pub image_cell_contract_address: H160,
}
