use crate::consts::{CHANNEL_CODE_HASH, CLIENT_TYPE_ARGS, CONNECTION_CODE_HASH};
use crate::generator::GENESIS_TXHASH;
use crate::rpc_client::RpcClient;

use anyhow::{anyhow, Result};
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
use ckb_types::H256;

use ibc_test_framework::prelude::{ChannelId, Wallet};
use ibc_test_framework::types::process::ChildProcess;
use relayer::chain::ckb::prelude::CkbReader;
use relayer::chain::ckb4ibc::extractor::{
    extract_channel_end_from_tx, extract_connections_from_tx,
};
use relayer::keyring::{Secp256k1AddressType, Secp256k1KeyPair};
use reqwest::blocking::Client;

use secp256k1::{PublicKey, Secp256k1, SecretKey};

use toml_edit::{value, Document};

use super::common::{gen_secp256k1_private_key, wait_task};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration;
use std::time::{self, Instant};
use std::{fs, thread};
use tendermint_rpc::Url;

#[derive(Deserialize)]
struct RpcResponse {
    pub result: H256,
}

impl std::fmt::Display for RpcResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", self.result))
    }
}

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

fn send_tx(request_body: String, port: u32) -> Result<RpcResponse> {
    // Create a reqwest client
    let client = Client::new();

    // Build the request with the appropriate headers and body
    let response = client
        .post(format!("http://localhost:{}", port))
        .header("content-type", "application/json")
        .body(request_body)
        .send()?
        .error_for_status()?;

    // Deserialize the response JSON into RpcResponse
    let response = response.text().unwrap();
    match serde_json::from_str::<RpcResponse>(&response) {
        Ok(value) => Ok(value),
        Err(_) => Err(anyhow!("{}", response)),
    }
}

fn modify_ckb_config_port(ckb_path: &Path, port: u32) -> Result<()> {
    fn replace_port(addr: &str, port: u32, sep: &str) -> String {
        let port_str = port.to_string();
        let mut parts: Vec<_> = addr.split(sep).collect();
        parts.pop();
        parts.push(&port_str);
        parts.join(sep)
    }

    fn replace_port_in_item(item: &mut toml_edit::Item, port: u32, sep: &str) {
        let addr = item.as_value().unwrap().as_str().unwrap();
        *item = value(replace_port(addr, port, sep));
    }

    // modify ckb.toml
    {
        let ckb_config_path = ckb_path.join("ckb.toml");
        let content = fs::read_to_string(&ckb_config_path)?;
        let mut ckb_config = content.parse::<Document>().expect("invalid toml");
        // rpc port
        replace_port_in_item(&mut ckb_config["rpc"]["listen_address"], port, ":");
        // network p2p port
        replace_port_in_item(
            &mut ckb_config["network"]["listen_addresses"][0],
            port + 1,
            "/",
        );
        fs::write(ckb_config_path, ckb_config.to_string())?;
    }

    // modify miner.toml
    {
        let miner_config_path = ckb_path.join("ckb-miner.toml");
        let content = fs::read_to_string(&miner_config_path)?;
        let mut miner_config = content.parse::<Document>().expect("invalid toml");
        // rpc port
        replace_port_in_item(&mut miner_config["miner"]["client"]["rpc_url"], port, ":");
        fs::write(miner_config_path, miner_config.to_string())?;
    }

    Ok(())
}

pub fn prepare_ckb_chain(
    ckb_path: &str,
    port: u32,
    genesis_wallets: &[(&Wallet, u64)],
) -> (ChildProcess, ChildProcess) {
    println!("\n========== Prepare Ckb node on port {port} ==========\n");

    let mut working_dir = std::env::current_dir().unwrap();
    working_dir.push(ckb_path);

    let _ = std::fs::remove_dir_all(ckb_path);
    std::fs::create_dir(ckb_path).unwrap();

    Command::new("ckb")
        .arg("init")
        .arg("--chain")
        .arg("dev")
        .current_dir(&working_dir)
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

    std::fs::copy(
        "../forcerelay-test/ckb/ckb.toml",
        format!("{}/ckb.toml", ckb_path),
    )
    .unwrap();
    std::fs::copy(
        "../forcerelay-test/ckb/dev.toml",
        format!("{}/specs/dev.toml", ckb_path),
    )
    .unwrap();
    std::fs::copy("ckb-miner.toml", format!("{}/ckb-miner.toml", ckb_path)).unwrap();

    // issue cells
    {
        let dev_spec_path = format!("{}/specs/dev.toml", ckb_path);
        let content = fs::read_to_string(&dev_spec_path).unwrap();
        let mut dev_spec: ChainSpec = toml::from_str(&content).expect("invalid spec");
        let issued_cell_template = dev_spec.genesis.issued_cells.last().unwrap().to_owned();

        for (wallet, _capacity) in genesis_wallets.iter() {
            let args = pubkey_to_script_args(&wallet.key.public_key);
            let mut cell = issued_cell_template.clone();
            cell.lock.args = JsonBytes::from_vec(args.to_vec());
            dev_spec.genesis.issued_cells.push(cell);
        }
        let content = toml::to_string_pretty(&dev_spec).unwrap();
        fs::write(dev_spec_path, content).unwrap();
    }

    modify_ckb_config_port(Path::new(ckb_path), port).unwrap();

    let ckb_process = ChildProcess::new(
        Command::new("ckb")
            .arg("run")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    wait_for_port(port);

    let miner_process = ChildProcess::new(
        Command::new("ckb")
            .arg("miner")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );

    // check transaction in genesis
    check_and_wait_ckb_transaction(GENESIS_TXHASH, port);

    let output = send_tx(
        fs::read_to_string("txs/deploy_connection.json").unwrap(),
        port,
    )
    .unwrap();
    println!("deploying connection: {output}");

    // check `txs/deploy_connection.json`
    check_and_wait_ckb_transaction(output.result, port);

    let output = send_tx(fs::read_to_string("txs/deploy_channel.json").unwrap(), port).unwrap();
    println!("deploying channel: {output}");

    // check `txs/deploy_channel.json`
    check_and_wait_ckb_transaction(output.result, port);

    let output = send_tx(
        fs::read_to_string("txs/deploy_packet_metadata.json").unwrap(),
        port,
    )
    .unwrap();
    println!("deploying packet and metadata: {output}");

    // check `txs/deploy_packet_metadata.json`
    check_and_wait_ckb_transaction(output.result, port);

    let output = send_tx(
        fs::read_to_string("txs/create_connection.json").unwrap(),
        port,
    )
    .unwrap();
    println!("deploying create connection: {output}");

    // check `txs/create_connection.json`
    check_and_wait_ckb_transaction(output.result, port);

    (ckb_process, miner_process)
}

fn pubkey_to_script_args(public_key: &PublicKey) -> [u8; 20] {
    ckb_hash::blake2b_256(public_key.serialize())[..20]
        .try_into()
        .unwrap()
}

fn wait_for_port(port: u32) {
    let timeout = Duration::from_secs(15);
    let now = Instant::now();
    while let Err(err) = wait_task(get_client(port).get_blockchain_info()) {
        if now.elapsed() > timeout {
            panic!(
                "wait for port {} timeout({:?}), error {:?}",
                port, timeout, err
            );
        }
    }
}

fn get_client(port: u32) -> RpcClient {
    let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
    RpcClient::new(&url, &url)
}

fn check_and_wait_ckb_transaction(hash: H256, port: u32) {
    let client = get_client(port);
    let mut loop_count = 0;
    loop {
        let result = wait_task(client.get_transaction(&hash)).unwrap();
        if let Some(tx) = result {
            if Status::Committed == tx.tx_status.status {
                return;
            }
        }
        if loop_count > 10 {
            panic!("cannot find tx_hash {hash} on port {port}");
        }
        loop_count += 1;
        thread::sleep(time::Duration::from_secs(1));
    }
}

pub fn fetch_ibc_connections(port: u32) -> IbcConnections {
    let client = get_client(port);
    let mut loop_count = 0;
    loop {
        let search_connection_cell = client.fetch_live_cells(
            SearchKey {
                script: Script::new_builder()
                    .code_hash(CONNECTION_CODE_HASH.pack())
                    .args("".pack()) // FIXME: use prefix search
                    .hash_type(ScriptHashType::Type.into())
                    .build()
                    .into(),
                script_type: ScriptType::Lock,
                filter: None,
                with_data: None,
                group_by_transaction: None,
                script_search_mode: None,
            },
            1,
            None,
        );
        let cells = wait_task(search_connection_cell).unwrap();
        if let Some(connection_cell) = cells.objects.into_iter().next() {
            let tx = wait_task(client.get_transaction(&connection_cell.out_point.tx_hash))
                .unwrap()
                .unwrap()
                .transaction
                .unwrap()
                .inner;
            let tx = match tx {
                ckb_jsonrpc_types::Either::Left(r) => r,
                ckb_jsonrpc_types::Either::Right(json_bytes) => {
                    serde_json::from_slice(json_bytes.as_bytes()).unwrap()
                }
            };
            let prefix = "forcerelay".as_bytes().to_vec().try_into().unwrap();
            let (_, ibc_connection) = extract_connections_from_tx(tx, &prefix).unwrap();
            return ibc_connection;
        } else {
            if loop_count > 30 {
                panic!("connection cell cannot find on port {port} for {loop_count}s");
            }
            loop_count += 1;
            thread::sleep(time::Duration::from_secs(1));
        }
    }
}

fn channel_id_to_u16(channel_id: &ChannelId) -> u16 {
    let channel_str = channel_id.to_string();
    let mut parts = channel_str.split('-');
    assert_eq!(parts.next().unwrap(), "channel", "expect prefix channel");
    let channel_id_num = parts.next().unwrap().parse().unwrap();
    assert!(parts.next().is_none(), "unknown parts in the string");
    channel_id_num
}

pub fn fetch_ibc_channel_cell(port: u32, port_id: [u8; 32], channel_id: &ChannelId) -> IbcChannel {
    let rpc_client = get_client(port);
    let mut loop_count = 0;
    loop {
        let search_channel_cell = rpc_client.fetch_live_cells(
            SearchKey {
                script: Script::new_builder()
                    .code_hash(CHANNEL_CODE_HASH.pack())
                    .args(
                        ChannelArgs {
                            client_id: CLIENT_TYPE_ARGS.into(),
                            open: true,
                            channel_id: channel_id_to_u16(channel_id),
                            port_id,
                        }
                        .to_args()
                        .pack(),
                    )
                    .hash_type(ScriptHashType::Type.into())
                    .build()
                    .into(),
                script_type: ScriptType::Lock,
                filter: None,
                with_data: None,
                group_by_transaction: None,
                script_search_mode: None,
            },
            1,
            None,
        );
        let cells = wait_task(search_channel_cell).unwrap();
        if let Some(channel_cell) = cells.objects.first() {
            let tx_hash = &channel_cell.out_point.tx_hash;
            let tx = wait_task(rpc_client.get_transaction(tx_hash))
                .unwrap()
                .unwrap()
                .transaction
                .unwrap()
                .inner;
            let tx = match tx {
                ckb_jsonrpc_types::Either::Left(r) => r,
                ckb_jsonrpc_types::Either::Right(json_bytes) => {
                    serde_json::from_slice(json_bytes.as_bytes()).unwrap()
                }
            };
            let (_, channel_end) = extract_channel_end_from_tx(tx).unwrap();
            return channel_end;
        } else {
            if loop_count > 30 {
                panic!("channel cell cannot find on port {port} for {loop_count}s");
            }
            loop_count += 1;
            thread::sleep(time::Duration::from_secs(1));
        }
    }
}

/// Add CKB devnet relayer wallet to the chain.
pub(crate) fn add_ckb_devnet_relayer_wallet(
    driver: &ibc_test_framework::prelude::ChainDriver,
    prefix: &str,
    use_random_id: bool,
) -> std::result::Result<ibc_test_framework::prelude::Wallet, ibc_test_framework::prelude::Error> {
    let private_key = {
        let data = hex::decode("ce513408c1f5117332abc926aac4f3ed28bc8e1fb21b7d5a3d65414374167d71")
            .unwrap();
        SecretKey::from_slice(&data).unwrap()
    };
    add_pk_to_ckb_wallet(driver, prefix, use_random_id, private_key)
}

/// Add a wallet to the chain.
pub(crate) fn add_ckb_wallet(
    driver: &ibc_test_framework::prelude::ChainDriver,
    prefix: &str,
    use_random_id: bool,
) -> std::result::Result<ibc_test_framework::prelude::Wallet, ibc_test_framework::prelude::Error> {
    // generate random secp256k1 private key
    let private_key = gen_secp256k1_private_key();
    add_pk_to_ckb_wallet(driver, prefix, use_random_id, private_key)
}

fn add_pk_to_ckb_wallet(
    _driver: &ibc_test_framework::prelude::ChainDriver,
    prefix: &str,
    _use_random_id: bool,
    private_key: SecretKey,
) -> std::result::Result<ibc_test_framework::prelude::Wallet, ibc_test_framework::prelude::Error> {
    // generate random secp256k1 private key
    let pk_name = format!("{}_ckb_wallet", prefix);

    // wallet address
    let public_key = private_key.public_key(&Secp256k1::new());
    let payload = AddressPayload::from_pubkey(&public_key);
    let address = Address::new(NetworkType::Dev, payload, true);

    let key =
        Secp256k1KeyPair::from_key_pair(private_key, public_key, Secp256k1AddressType::Ckb, prefix)
            .expect("construct key pair");
    let wallet = Wallet::new(pk_name, address.to_string(), key);

    Ok(wallet)
}
