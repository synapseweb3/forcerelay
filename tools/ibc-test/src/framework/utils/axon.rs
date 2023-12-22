use anyhow::{anyhow, bail, Context, Result};

use ethers::providers::{Http, Middleware, Provider};
use ethers::signers::Signer;

use ibc_test_framework::prelude::*;
use ibc_test_framework::types::axon::DeployedContracts;
use ibc_test_framework::types::process::ChildProcess;
use ibc_test_framework::util::random::random_u32;

use relayer::keyring::{Secp256k1AddressType, Secp256k1KeyPair};

use secp256k1::{Secp256k1, SecretKey};

use toml_edit::{value, Document};

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
    pre_mint: Option<(&Wallet, u64)>,
) -> Result<(ChildProcess, Denom)> {
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
    let binding = working_dir.join("devtools/chain/specs/single_node");
    let chain_spec_dir_path = binding.to_str().unwrap();
    std::fs::create_dir_all(chain_spec_dir_path)
        .with_context(|| format!("create_chain_spec_dir {:?}", chain_spec_dir_path))?;

    // copy configs to working dir
    for file in [
        "config.toml",
        "specs/single_node/chain-spec.toml",
        "default.db-options",
        "bls.key",
        "net.key",
    ] {
        let src_path = axon_src_path.join("devtools/chain").join(file);
        std::fs::copy(&src_path, working_dir.join("devtools/chain").join(file))
            .with_context(|| format!("cp {:?} -> {:?}", &src_path, working_dir.join(file)))?;
    }

    let chain_config_path = working_dir.join("devtools/chain").join("config.toml");
    let chain_spec_path = working_dir
        .join("devtools/chain")
        .join("specs/single_node/chain-spec.toml");

    // Modify configs
    let mut config_doc = fs::read_to_string(&chain_config_path)
        .with_context(|| format!("read chain config from {:?}", &chain_config_path))?
        .parse::<Document>()
        .expect("invalid toml");
    // modify ports
    config_doc["rpc"]["http_listening_address"] = value(format!("0.0.0.0:{}", port));
    config_doc["rpc"]["ws_listening_address"] = value(format!("0.0.0.0:{}", port + 1));
    config_doc["network"]["listening_address"] = value(format!("/ip4/0.0.0.0/tcp/{}", port + 2));
    *config_doc["network"]["bootstraps"].get_mut(0).unwrap() = value(format!(
        "/ip4/127.0.0.1/tcp/{}/p2p/QmNk6bBwkLPuqnsrtxpp819XLZY3ymgjs3p1nKtxBVgqxj",
        port + 2
    ));
    fs::write(&chain_config_path, config_doc.to_string())
        .with_context(|| format!("write config to {:?}", &chain_config_path))?;

    // init axon
    let _init_command = Command::new("axon")
        .arg("init")
        .arg("--config")
        .arg(&chain_config_path)
        .arg("--chain-spec")
        .arg(&chain_spec_path)
        .current_dir(&working_dir)
        .output()?;

    // start process
    let chain_process = ChildProcess::new(
        Command::new("axon")
            .arg("run")
            .arg("--config")
            .arg(&chain_config_path)
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    wait_for_port(port);

    // Deploy IBC contract
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
    let transfer_contract_address =
        parsing_contract_address_from_output(&output, "ICS20TransferERC20")?;

    println!("ibc handler deployed at {:#x}", contract_address);

    // deploy test ERC20 and mint token
    let token_name = "AT";
    let token_symbol = "AT";
    let mut cmd = Command::new("yarn");
    cmd.current_dir(&ibc_contracts_src_path)
        .arg("truffle")
        .arg("exec")
        .arg("--network")
        .arg("axon")
        .arg("scripts/deploy_erc20.js")
        .env("AXON_HTTP_RPC_URL", format!("http://localhost:{port}"))
        .env("TOKEN_NAME", token_name)
        .env("TOKEN_SYMBOL", token_symbol);
    if let Some((mint_to, mint_amount)) = pre_mint {
        cmd.env("MINT_TO", mint_to.address.as_str())
            .env("MINT_AMOUNT", mint_amount.to_string());
    }
    let output = cmd.output().with_context(|| "yarn truffle")?;
    // get contract address from output
    check_command_output(&output, &working_dir)?;
    let output = String::from_utf8(output.stdout.clone())?;
    let token_address = parsing_contract_address_from_output(&output, "SimpleToken")?;
    let denom = Denom::base(&format!("{token_address:#x}"));

    println!("test token deployed at {:#x}", token_address);

    // write deployment info
    let deployment = DeployedContracts {
        contract_address,
        transfer_contract_address,
    };
    let path = working_dir.join(AXON_CONTRACTS_CONFIG_PATH);
    std::fs::write(path, toml::to_string(&deployment)?).with_context(|| "write deployment info")?;

    Ok((chain_process, denom))
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
        let data = hex::decode("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
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
