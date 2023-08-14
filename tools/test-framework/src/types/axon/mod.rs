use ethers::types::H160;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DeployedContracts {
    pub contract_address: H160,
    pub image_cell_contract_address: H160,
    pub ckb_light_client_contract_address: H160,
}
