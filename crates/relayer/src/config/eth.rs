use std::str::FromStr;

use ibc_relayer_types::{
    clients::ics07_eth::types::{FixedVector, Fork, Forks, H256, U4},
    core::ics24_host::identifier::ChainId,
};
use serde_derive::{Deserialize, Serialize};
use tendermint_rpc::Url;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EthChainConfig {
    pub id: ChainId,
    pub genesis_time: u64,
    #[serde(default)]
    pub genesis_root: H256,
    pub websocket_addr: Url,
    #[serde(default)]
    pub initial_checkpoint: [u8; 32],
    pub key_name: String,
    pub rpc_addr: String,
    pub rpc_port: u16,
    #[serde(default)]
    pub forks: Forks,
    pub max_checkpoint_age: u64,
}

impl EthChainConfig {
    pub fn mainnet() -> Self {
        todo!()
    }

    pub fn fork_version(&self, slot: u64) -> FixedVector<u8, U4> {
        let epoch = slot / 32;

        if epoch >= self.forks.bellatrix.epoch {
            self.forks.bellatrix.fork_version.clone()
        } else if epoch >= self.forks.altair.epoch {
            self.forks.altair.fork_version.clone()
        } else {
            self.forks.genesis.fork_version.clone()
        }
    }

    pub fn goerli() -> Self {
        Self {
            id: ChainId::new(String::from("5"), 1),
            genesis_time: 1616508000,
            genesis_root: <[u8; 32]>::try_from(
                hex::decode("043db0d9a83813551ee2f33450d23797757d430911a9320530ad8a0eabc43efb")
                    .unwrap(),
            )
            .unwrap()
            .into(),
            websocket_addr: Url::from_str("http://www.dummy.com").unwrap(),
            rpc_addr: Default::default(),
            rpc_port: 8545,
            forks: Forks {
                genesis: Fork {
                    epoch: 0,
                    fork_version: hex::decode("00001020").unwrap().into(),
                },
                altair: Fork {
                    epoch: 36660,
                    fork_version: hex::decode("01001020").unwrap().into(),
                },
                bellatrix: Fork {
                    epoch: 112260,
                    fork_version: hex::decode("02001020").unwrap().into(),
                },
            },
            max_checkpoint_age: 1_209_600,
            initial_checkpoint: Default::default(),
            key_name: Default::default(),
        }
    }
}
