use crate::consts::{CHANNEL_CODE_HASH, CLIENT_TYPE_ARGS, CONNECTION_CODE_HASH};
use crate::rpc_client::RpcClient;

use anyhow::Result;
use ckb_chain_spec::ChainSpec;

use ckb_ics_axon::handler::{IbcChannel, IbcConnections};
use ckb_ics_axon::object::State;
use ckb_ics_axon::ChannelArgs;
use ckb_jsonrpc_types::{ChainInfo, Deserialize, JsonBytes, Status};
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_sdk::*;
use ckb_types::core::ScriptHashType;
use ckb_types::packed::Script;
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::{h256, H256};
use ibc_test_framework::prelude::Wallet;
use ibc_test_framework::types::process::ChildProcess;
use relayer::chain::ckb::prelude::CkbReader;
use relayer::chain::ckb4ibc::extractor::{
    extract_channel_end_from_tx, extract_connections_from_tx,
};
use relayer::keyring::{Secp256k1AddressType, Secp256k1KeyPair};
use secp256k1::rand::Rng;
use secp256k1::{rand, PublicKey, Secp256k1, SecretKey};

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

fn send_tx(req: &str, port: u32) -> RpcResponse {
    let output = Command::new("curl")
        .arg("-H")
        .arg("content-type: application/json")
        .arg("-d")
        .arg(format!("@{}", req))
        .arg(format!("http://localhost:{}", port))
        .output()
        .unwrap();
    serde_json::from_slice(&output.stdout)
        .map_err(|e| {
            println!("{}", String::from_utf8_lossy(&output.stdout));
            e
        })
        .unwrap()
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

    if port != 8114 {
        Command::new("sed")
            .arg("-i")
            .arg("")
            .arg("-e")
            .arg(format!("s/8114/{}/g", port))
            .arg("-e")
            .arg(format!("s/8115/{}/g", port + 1))
            .arg("ckb.toml")
            .current_dir(&working_dir)
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
        Command::new("sed")
            .arg("-i")
            .arg("")
            .arg("-e")
            .arg(format!("s/8114/{}/g", port))
            .arg("ckb-miner.toml")
            .current_dir(&working_dir)
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

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
    check_and_wait_ckb_transacton(
        h256!("0x227de871ce6ab120a67960f831b04148bf79b4e56349dde7a8001f93191736ed"),
        port,
    );

    let output = send_tx("txs/deploy_conn_chan.json", port);
    print!("deploying conn and channel: {output}");

    // check `txs/deploy_conn_chan.json`
    check_and_wait_ckb_transacton(output.result, port);

    let output = send_tx("txs/deploy_packet_metadata.json", port);
    print!("deploying packet and metadata: {output}");

    // check `txs/deploy_packet_metadata.json`
    check_and_wait_ckb_transacton(output.result, port);

    let output = send_tx("txs/create_connection.json", port);
    print!("deploying create connection: {output}");

    // check `txs/create_connection.json`
    check_and_wait_ckb_transacton(output.result, port);

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
    while let Err(err) = get_blockchain_info(port) {
        if now.elapsed() > timeout {
            panic!(
                "wait for port {} timeout({:?}), error {:?}",
                port, timeout, err
            );
        }
    }
}

fn get_blockchain_info(port: u32) -> Result<ChainInfo> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
    let client = RpcClient::new(&url, &url);
    rt.block_on(client.get_blockchain_info())
        .map_err(Into::into)
}

fn check_and_wait_ckb_transacton(hash: H256, port: u32) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
    let client = RpcClient::new(&url, &url);
    let mut loop_count = 0;
    loop {
        let result = rt.block_on(client.get_transaction(&hash)).unwrap();
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
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
    let client = RpcClient::new(&url, &url);
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
            },
            1,
            None,
        );
        let cells = rt.block_on(search_connection_cell).unwrap();
        if let Some(connection_cell) = cells.objects.into_iter().next() {
            let tx = rt
                .block_on(client.get_transaction(&connection_cell.out_point.tx_hash))
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

pub fn fetch_ibc_channel_cell(port: u32, port_id: [u8; 32]) -> IbcChannel {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
    let rpc_client = RpcClient::new(&url, &url);
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
                            channel_id: 0,
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
            },
            1,
            None,
        );
        let cells = rt.block_on(search_channel_cell).unwrap();
        if let Some(channel_cell) = cells.objects.first() {
            let tx_hash = &channel_cell.out_point.tx_hash;
            let tx = rt
                .block_on(rpc_client.get_transaction(tx_hash))
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

fn gen_secp256k1_private_key() -> SecretKey {
    let mut rng = rand::thread_rng();
    let mut private_key = [0u8; 32];
    rng.fill(&mut private_key);
    SecretKey::from_slice(&private_key).unwrap()
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
