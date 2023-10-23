use ckb_sdk::AddressPayload;
use ckb_types::packed::Script;
use ckb_types::prelude::Entity;
use futures::Future;
use ibc_test_framework::prelude::*;
use relayer::chain::ChainType;
use secp256k1::Secp256k1;
use secp256k1::{
    rand::{self, Rng},
    SecretKey,
};
use std::path::PathBuf;
use std::str::FromStr;
use tokio::runtime::Runtime;

use crate::generator::PRIVKEY;

fn get_rt() -> &'static Runtime {
    lazy_static::lazy_static! {
        static ref RT: Runtime = Runtime::new().unwrap();
    }
    &RT
}

pub fn wait_task<F: Future>(f: F) -> F::Output {
    get_rt().block_on(f)
}

pub fn gen_secp256k1_private_key() -> SecretKey {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);
    SecretKey::from_slice(&private_key).unwrap()
}

pub fn get_chain_type(command_path: &str) -> ChainType {
    let path: PathBuf = command_path.into();
    match path.file_name().unwrap().to_str().unwrap() {
        "ckb" => ChainType::Ckb,
        "axon" => ChainType::Axon,
        chain => unimplemented!("unknown chain {:?}", chain),
    }
}

pub fn transfer_port_id(chain_type: ChainType) -> PortId {
    match chain_type {
        ChainType::Ckb => {
            // CKB only allow h256 as portId
            let relayer_key = SecretKey::from_str(PRIVKEY).unwrap();
            let address =
                AddressPayload::from_pubkey(&relayer_key.public_key(&Secp256k1::default()));
            let script: Script = (&address).into();
            let script_hash = script.calc_script_hash();
            PortId::from_str(&hex::encode(script_hash.as_slice())).unwrap()
        }
        ChainType::Axon => {
            // Axon default port ID
            PortId::from_str("transfer").unwrap()
        }
        _ => {
            unreachable!()
        }
    }
}
