use crate::consts::{CHANNEL_CODE_HASH, CLIENT_TYPE_ARGS, CONNECTION_CODE_HASH};
use crate::rpc_client::RpcClient;

use anyhow::{anyhow, bail, Result};
use ckb_chain_spec::ChainSpec;

use ckb_ics_axon::handler::{IbcChannel, IbcConnections};
use ckb_ics_axon::object::State;
use ckb_ics_axon::ChannelArgs;
use ckb_jsonrpc_types::{Deserialize, JsonBytes, Status};
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_sdk::*;
use ckb_types::core::ScriptHashType;
use ckb_types::packed::Script;
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::{h256, H256};
use ethers::providers::{Http, Middleware, Provider, Ws};
use ethers::signers::Signer;
use futures::Future;
use ibc_test_framework::prelude::*;
use ibc_test_framework::types::axon::DeployedContracts;
use ibc_test_framework::types::process::ChildProcess;
use ibc_test_framework::util::random::random_u32;
use relayer::chain::ckb::prelude::CkbReader;
use relayer::chain::ckb4ibc::extractor::{
    extract_channel_end_from_tx, extract_connections_from_tx,
};
use relayer::keyring::{KeyRing, Secp256k1AddressType, Secp256k1KeyPair, Store};
use reqwest::blocking::Client;
use secp256k1::rand::{thread_rng, Rng};
use secp256k1::{rand, PublicKey, Secp256k1, SecretKey};
use tokio::runtime::Runtime;
use toml_edit::{value, Document, Item, Table};

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;
use std::time::{self, Instant};
use std::{fs, thread};
use tendermint_rpc::Url;

use super::common::{gen_secp256k1_private_key, wait_task};

const AXON_CONTRACTS_CONFIG_PATH: &str = "deployed_contracts.toml";
const AXON_SRC_PATH: &str = "AXON_SRC_PATH";
const IBC_CONTRACTS_SRC_PATH: &str = "IBC_CONTRACTS_SRC_PATH";

pub fn check_channel(channel: &IbcChannel) -> bool {
    channel.state == State::Open
}

pub fn check_ibc_connection(connection: IbcConnections, count: u32) -> bool {
    if connection.connections.len() != count as usize {
        return false;
    }
    // FIXME @jjy in current implementation,
    // a failed init conn is remained in the vector, so we must check last opened conn
    let connection = connection.connections.into_iter().last().unwrap();
    connection.state == State::Open
}

pub fn prepare_axon_chain(
    dir_path: &str,
    port: u32,
    genesis_wallets: &[(&Wallet, u64)],
) -> Result<ChildProcess> {
    println!("\n========== Prepare Axon node on port {port} ==========\n");

    // read envs
    let axon_src_path: PathBuf = std::env::var(AXON_SRC_PATH)
        .expect("Get AXON_SRC_PATH")
        .into();

    let ibc_contracts_src_path: PathBuf = std::env::var(IBC_CONTRACTS_SRC_PATH)
        .expect("Get IBC_CONTRACTS_SRC_PATH")
        .into();

    // prepare working dir
    let mut working_dir = std::env::current_dir().unwrap();
    working_dir.push(dir_path);
    let _ = std::fs::remove_dir_all(dir_path);
    std::fs::create_dir(dir_path).unwrap();

    // copy configs to working dir
    for file in [
        "config.toml",
        "genesis_single_node.json",
        "default.db-options",
    ] {
        let src_path = axon_src_path.join("devtools/chain").join(file);
        std::fs::copy(src_path, working_dir.join(file))?;
    }

    let chain_config_path = working_dir.join("config.toml");
    let genesis_config_path = working_dir.join("genesis_single_node.json");

    // Modify configs

    let mut config_doc = fs::read_to_string(&chain_config_path)?
        .parse::<Document>()
        .expect("invalid toml");
    // modify ports
    config_doc["rpc"]["http_listening_address"] = value(format!("0.0.0.0:{}", port));
    config_doc["rpc"]["ws_listening_address"] = value(format!("0.0.0.0:{}", port + 1));
    config_doc["network"]["listening_address"] = value(format!("/ip4/0.0.0.0/tcp/{}", port + 2));
    *config_doc["network"]["bootstraps"].get_mut(0).unwrap() = value(&format!(
        "/ip4/127.0.0.1/tcp/{}/p2p/QmNk6bBwkLPuqnsrtxpp819XLZY3ymgjs3p1nKtxBVgqxj",
        port + 2
    ));

    // genesis wallets
    for (wallet, balance) in genesis_wallets {
        let mut item = Table::new();
        item.entry("address")
            .or_insert(value(wallet.address.as_str()));
        item.entry("balance")
            .or_insert(value(hex::encode(balance.to_be_bytes())));
        config_doc["accounts"]
            .as_array_of_tables_mut()
            .unwrap()
            .push(item);
    }

    fs::write(&chain_config_path, config_doc.to_string())?;

    // start process
    let chain_process = ChildProcess::new(
        Command::new("axon")
            .arg("run")
            .arg("-c")
            .arg(&chain_config_path)
            .arg("-g")
            .arg(&genesis_config_path)
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    wait_for_port(port);

    // Deploy IBC contract
    {
        println!("deploying ibc contracts",);
        let output = Command::new("yarn")
            .arg("migrate")
            .env("AXON_HTTP_RPC_URL", format!("http://localhost:{}", port))
            .current_dir(&ibc_contracts_src_path)
            .output()?;
        // get contract address from output
        let contract_address: Option<ethers::types::H160> = if output.status.success() {
            let output = String::from_utf8(output.stdout.clone())?;
            output
                .lines()
                .filter(|line| line.starts_with("Done Deployment OwnableIBCHandler"))
                .map(|line| {
                    line.split("at").last().map(|s| {
                        let s = s.trim().trim_start_matches("0x");
                        let bytes = hex::decode(s).expect("decode hex address");
                        ethers::types::H160::from_slice(&bytes)
                    })
                })
                .flatten()
                .next()
        } else {
            None
        };

        if contract_address.is_none() {
            let log_path = working_dir.join("deploy.log");
            let err_log_path = working_dir.join("deploy.err.log");
            fs::write(&log_path, output.stdout)?;
            fs::write(&err_log_path, output.stderr)?;
            bail!(
                "failed to parse deployed contract address from logs: {}, log: {}, err log: {}",
                output.status.to_string(),
                log_path.to_string_lossy(),
                err_log_path.to_string_lossy()
            );
        }
        let contract_address = contract_address.unwrap();
        println!("ibc handler deployed at {:#x}", contract_address);

        // write deployment info
        let deployment = DeployedContracts {
            contract_address,
            image_cell_contract_address: ethers::types::H160::default(),
            ckb_light_client_contract_address: ethers::types::H160::default(),
        };
        let path = working_dir.join(AXON_CONTRACTS_CONFIG_PATH);
        std::fs::write(path, toml::to_string(&deployment)?)?;
    }

    Ok(chain_process)
}

fn wait_for_port(port: u32) {
    let timeout = Duration::from_secs(15);
    let now = Instant::now();
    while let Err(err) = wait_task(async {
        let client = get_client(port).map_err(|err| anyhow!(err))?;
        client.get_chainid().await.map_err(|err| anyhow!(err))
    }) {
        if now.elapsed() > timeout {
            panic!(
                "wait for port {} timeout({:?}), error {:?}",
                port, timeout, err
            );
        }
    }
}

fn get_client(port: u32) -> Result<Provider<Http>> {
    let url = format!("http://127.0.0.1:{}", port);
    let client = Provider::<Http>::try_from(url)?;
    Ok(client)
}

pub(crate) fn add_axon_wallet(
    driver: &ibc_test_framework::prelude::ChainDriver,
    wallet_id: String,
) -> Result<Wallet, Error> {
    // generate random secp256k1 private key
    let private_key = gen_secp256k1_private_key();
    // wallet address
    let public_key = private_key.public_key(&Secp256k1::new());
    let key = Secp256k1KeyPair::from_key_pair(
        private_key,
        public_key,
        Secp256k1AddressType::Axon,
        &wallet_id,
    )
    .expect("construct key pair");
    let address = ethers::signers::Wallet::from_bytes(&key.private_key.secret_bytes())
        .unwrap()
        .address();
    let address = format!("{:#x}", address);
    let wallet = Wallet::new(wallet_id, address, key);
    Ok(wallet)
}

/// Add Axon devnet relayer wallet to the chain.
pub(crate) fn add_axon_devnet_relayer_wallet(
    driver: &ChainDriver,
    prefix: &str,
    use_random_id: bool,
) -> Result<Wallet, Error> {
    let wallet_id = if use_random_id {
        let num = random_u32();
        format!("{prefix}-{num:x}")
    } else {
        prefix.to_string()
    };
    let private_key = {
        let data = hex::decode("37aa0f893d05914a4def0460c0a984d3611546cfb26924d7a7ca6e0db9950a2d")
            .unwrap();
        SecretKey::from_slice(&data).unwrap()
    };
    // wallet address
    let public_key = private_key.public_key(&Secp256k1::new());
    let key = Secp256k1KeyPair::from_key_pair(
        private_key,
        public_key,
        Secp256k1AddressType::Axon,
        &wallet_id,
    )
    .expect("construct key pair");
    let address = ethers::signers::Wallet::from_bytes(&key.private_key.secret_bytes())
        .unwrap()
        .address();
    let address = format!("{:#x}", address);
    let wallet = Wallet::new(wallet_id, address, key);
    Ok(wallet)
}
