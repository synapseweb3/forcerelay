use ckb_sdk::AddressPayload;
use ckb_types::packed::Script;
use ckb_types::prelude::Entity;
use ethers::core::k256::pkcs8::der::Writer;
use futures::Future;
use ibc_test_framework::prelude::*;
use relayer::chain::ChainType;
use secp256k1::rand::rngs;
use secp256k1::Secp256k1;
use secp256k1::{
    rand::{self, Rng},
    SecretKey,
};
use std::path::PathBuf;
use std::process::{Child, Command};
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

pub fn prepare_cell_emitter(
    axon_port: u16,
    ckb_port: u16,
) -> Result<(Child, std::sync::mpsc::Sender<()>), Error> {
    let listen_port = rngs::OsRng.gen_range(9000..10000);
    let private_path = std::env::current_dir()
        .unwrap()
        .join(format!("emitter-privkey-{listen_port}"));
    std::fs::File::create(&private_path)
        .map_err(|err| eyre!("failed to create emitter private file: {err}"))?
        .write(
            &hex::decode("37aa0f893d05914a4def0460c0a984d3611546cfb26924d7a7ca6e0db9950a2d")
                .unwrap(),
        )
        .unwrap();
    let store_path = std::env::current_dir()
        .unwrap()
        .join(format!("emitter-store-{listen_port}"));
    std::fs::create_dir_all(&store_path)
        .map_err(|err| eyre!("failed to create emitter store path: {err}"))?;
    let emitter_thread = Command::new("emitter")
        .arg("-c")
        .arg(format!("http://127.0.0.1:{ckb_port}"))
        .arg("--i")
        .arg(format!("http://127.0.0.1:{axon_port}"))
        .arg("-l")
        .arg(format!("127.0.0.1:{listen_port}"))
        .arg("-s")
        .arg(store_path)
        .arg("-p")
        .arg(private_path)
        .spawn()
        .map_err(|err| eyre!("failed to start emitter: {err}"))?;
    // check header sync progress
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        let output = Command::new("curl")
            .arg("-H")
            .arg("content-type: application/json")
            .arg("-d")
            .arg("{\"id\": 2, \"jsonrpc\": \"2.0\", \"method\": \"info\", \"params\": [] }")
            .arg(format!("http://127.0.0.1:{listen_port}"))
            .output()
            .unwrap();
        let log = if output.status.success() {
            output.stdout
        } else {
            output.stderr
        };
        println!("\n[CellEmitter] {}", String::from_utf8(log).unwrap());
        if rx.try_recv().is_ok() {
            return;
        }
    });
    Ok((emitter_thread, tx))
}
