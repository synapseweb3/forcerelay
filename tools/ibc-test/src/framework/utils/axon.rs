use anyhow::{anyhow, bail, Context, Result};

use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::Signer;

use ibc_test_framework::prelude::*;
use ibc_test_framework::types::axon::DeployedContracts;
use ibc_test_framework::types::process::ChildProcess;
use ibc_test_framework::util::random::random_u32;

use relayer::keyring::{Secp256k1AddressType, Secp256k1KeyPair};

use secp256k1::{Secp256k1, SecretKey};

use toml_edit::{value, Document, Table};

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use std::fs;
use std::time::Duration;
use std::time::Instant;

use super::common::{gen_secp256k1_private_key, wait_task};

const AXON_CONTRACTS_CONFIG_PATH: &str = "deployed_contracts.toml";
const AXON_SRC_PATH: &str = "AXON_SRC_PATH";
const IBC_CONTRACTS_SRC_PATH: &str = "IBC_CONTRACTS_SRC_PATH";

pub(crate) fn prepare_axon_chain(
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
    std::fs::create_dir_all(dir_path).with_context(|| format!("create_dir {:?}", dir_path))?;

    // copy configs to working dir
    for file in [
        "config.toml",
        "genesis_single_node.json",
        "default.db-options",
    ] {
        let src_path = axon_src_path.join("devtools/chain").join(file);
        std::fs::copy(&src_path, working_dir.join(file))
            .with_context(|| format!("cp {:?} -> {:?}", &src_path, working_dir.join(file)))?;
    }

    let chain_config_path = working_dir.join("config.toml");
    let genesis_config_path = working_dir.join("genesis_single_node.json");

    // Modify configs

    let mut config_doc = fs::read_to_string(&chain_config_path)
        .with_context(|| format!("read chain config from {:?}", &chain_config_path))?
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

    fs::write(&chain_config_path, config_doc.to_string())
        .with_context(|| format!("write config to {:?}", &chain_config_path))?;

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
            .output()
            .with_context(|| "yarn migrate")?;
        // get contract address from output
        check_command_output(&output, &working_dir)?;
        let output = String::from_utf8(output.stdout.clone())?;
        let contract_address = parsing_contract_address_from_output(&output, "OwnableIBCHandler")?;
        let mock_transfer_contract_address =
            parsing_contract_address_from_output(&output, "MockTransfer")?;
        let transfer_contract_address =
            parsing_contract_address_from_output(&output, "ICS20TransferERC20")?;

        println!("ibc handler deployed at {:#x}", contract_address);

        // write deployment info
        let deployment = DeployedContracts {
            contract_address,
            mock_transfer_contract_address,
            transfer_contract_address,
            image_cell_contract_address: ethers::types::H160::default(),
            ckb_light_client_contract_address: ethers::types::H160::default(),
        };
        let path = working_dir.join(AXON_CONTRACTS_CONFIG_PATH);
        std::fs::write(path, toml::to_string(&deployment)?)
            .with_context(|| "write deployment info")?;
    }

    Ok(chain_process)
}

fn check_command_output(output: &Output, working_dir: &Path) -> Result<()> {
    if !output.status.success() {
        let log_path = working_dir.join("deploy.log");
        let err_log_path = working_dir.join("deploy.err.log");
        fs::write(&log_path, &output.stdout)?;
        fs::write(&err_log_path, &output.stderr)?;
        bail!(
            "command return error status: {}, log: {}, err log: {}",
            output.status.to_string(),
            log_path.to_string_lossy(),
            err_log_path.to_string_lossy()
        );
    }
    Ok(())
}

fn parsing_contract_address_from_output(
    output: &str,
    contract: &str,
) -> Result<ethers::types::H160> {
    output
        .lines()
        .filter(|line| line.starts_with(format!("Done Deployment {contract}").as_str()))
        .filter_map(|line| {
            line.split("at").last().map(|s| {
                let s = s.trim().trim_start_matches("0x");
                let bytes = hex::decode(s).expect("decode hex address");
                ethers::types::H160::from_slice(&bytes)
            })
        })
        .next()
        .ok_or(anyhow!("failed to parsing {}", contract))
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
    _driver: &ibc_test_framework::prelude::ChainDriver,
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
    _driver: &ChainDriver,
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
