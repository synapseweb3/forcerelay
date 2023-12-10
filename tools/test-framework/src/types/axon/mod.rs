use ethers::types::H160;
use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DeployedContracts {
    pub contract_address: H160,
    pub transfer_contract_address: H160,
}
