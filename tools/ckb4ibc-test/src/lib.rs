// todo: refactor ckb and ckb4ibc rpc client
#[cfg(test)]
mod generator;
#[cfg(test)]
mod rpc_client;

#[cfg(test)]
mod tests {
    use super::rpc_client::RpcClient;
    use ckb_hash::blake2b_256;
    use ckb_ics_axon::handler::{IbcChannel, IbcConnections};
    use ckb_ics_axon::object::State;
    use ckb_ics_axon::ChannelArgs;
    // use ckb_sdk::constants::TYPE_ID_CODE_HASH;
    use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
    use ckb_types::core::ScriptHashType;
    use ckb_types::packed::Script;
    use ckb_types::prelude::{Builder, Entity, Pack};
    use ckb_types::{h256, H256};
    use relayer::chain::ckb::prelude::CkbReader;
    use relayer::chain::ckb4ibc::extractor::{
        extract_channel_end_from_tx, extract_connections_from_tx,
    };
    use secp256k1::{Secp256k1, SecretKey};
    use std::process::{Child, Command, Stdio};
    use std::str::FromStr;
    use std::thread;
    use std::time;
    use std::time::Duration;
    use tendermint_rpc::Url;

    const CONNECTION_CODE_HASH: H256 =
        h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
    const CHANNEL_CODE_HASH: H256 =
        h256!("0x9ea73e5003f580eb4f380944b1de0711c6b5a4bb96c6f9bf8186203b7c684606");
    const CLIENT_TYPE_ARGS: H256 =
        h256!("0x29866e133f707f070459b905065294ab1a7b70bea200952a080f849319ae6202");
    #[test]
    fn test_config() {
        use relayer::config::load;

        let path = "config.toml";
        load(path).unwrap();
    }

    #[ignore]
    #[test]
    fn integration_test() {
        prepare_ckb_chain("ckb-dev-a", 8114);
        prepare_ckb_chain("ckb-dev-b", 8214);

        thread::sleep(time::Duration::from_secs(10));

        Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("ibc-relayer-cli")
            .arg("--")
            .arg("--config")
            .arg("./tools/ckb4ibc-test/config.toml")
            .arg("create")
            .arg("connection")
            .arg("--a-chain")
            .arg("ckb4ibc-0")
            .arg("--b-chain")
            .arg("ckb4ibc-1")
            // .arg("--a-client")
            // .arg(format!("{CLIENT_TYPE_ARGS:x}"))
            // .arg("--b-client")
            // .arg(format!("{CLIENT_TYPE_ARGS:x}"))
            .current_dir("../../")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        let a_connection = fetch_ibc_connections(8114);
        println!("a_connection: {a_connection:?}");
        let b_connection = fetch_ibc_connections(8214);
        println!("b_connection: {b_connection:?}");
        if !check_ibc_connection(a_connection) || !check_ibc_connection(b_connection) {
            panic!("create connection failed");
        }

        let user_a_private_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let user_a_public_key = user_a_private_key.public_key(&Secp256k1::new()).serialize();
        let port_id_a = H256::from(blake2b_256(&user_a_public_key[..]));

        let user_b_private_key = SecretKey::from_slice(&[2u8; 32]).unwrap();
        let user_b_public_key = user_b_private_key.public_key(&Secp256k1::new()).serialize();
        let port_id_b = H256::from(blake2b_256(&user_b_public_key[..]));

        Command::new("cargo")
            .arg("run")
            .arg("-p")
            .arg("ibc-relayer-cli")
            .arg("--")
            .arg("--config")
            .arg("./tools/ckb4ibc-test/config.toml")
            .arg("create")
            .arg("channel")
            .arg("--a-port")
            .arg(format!("{:x}", port_id_a))
            .arg("--b-port")
            .arg(format!("{:x}", port_id_b))
            .arg("--a-chain")
            .arg("ckb4ibc-0")
            .arg("--a-connection")
            .arg("ckb4ibc-connection-0")
            .current_dir("../../")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        let a_channel = fetch_ibc_channel_cell(8114, port_id_a.into());
        println!("a_channel: {a_channel:?}");
        let b_channel = fetch_ibc_channel_cell(8214, port_id_b.into());
        println!("b_channel: {b_channel:?}");
        if !check_channel(&a_channel) || !check_channel(&b_channel) {
            panic!("create channel failed")
        }
    }

    fn check_channel(channel: &IbcChannel) -> bool {
        channel.state == State::Open
    }

    fn check_ibc_connection(connection: IbcConnections) -> bool {
        if connection.connections.len() != 1 {
            return false;
        }
        let connection = connection.connections.into_iter().next().unwrap();
        connection.state == State::Open
    }

    fn send_tx(req: &str, port: u32) {
        Command::new("curl")
            .arg("-H")
            .arg("content-type: application/json")
            .arg("-d")
            .arg(format!("@{}", req))
            .arg(format!("http://localhost:{}", port))
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

    fn prepare_ckb_chain(ckb_path: &str, port: u32) -> (Child, Child) {
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
            .unwrap();

        thread::sleep(Duration::from_secs(2));

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

        if port != 8114 {
            Command::new("sed")
                .arg("-i")
                .arg("")
                .arg("-e")
                .arg(format!("s/8114/{}/g", port))
                .arg("-e")
                .arg(format!("s/8115/{}/g", port + 1))
                // .arg(format!("{}/ckb.toml", ckb_path))
                .arg("ckb.toml")
                .current_dir(&working_dir)
                .spawn()
                .unwrap();
            Command::new("sed")
                .arg("-i")
                .arg("")
                .arg("-e")
                .arg(format!("s/8114/{}/g", port))
                // .arg(format!("{}/ckb-miner.toml", ckb_path))
                .arg("ckb-miner.toml")
                .current_dir(&working_dir)
                .spawn()
                .unwrap();
        }

        let ckb_run = Command::new("ckb")
            .arg("run")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        thread::sleep(Duration::from_secs(4));

        let ckb_miner = Command::new("ckb")
            .arg("miner")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        thread::sleep(Duration::from_secs(4));

        let prelude1 = "txs/deploy_conn_chan.json";
        println!("deploying conn and channel");
        send_tx(prelude1, port);

        thread::sleep(time::Duration::from_secs(3));

        let prelude2 = "txs/deploy_packet_metadata.json";
        println!("deploying packet and metadata");
        send_tx(prelude2, port);

        thread::sleep(time::Duration::from_secs(3));

        let create_connection_tx = "txs/create_connection.json";
        println!("deploying create connection");
        send_tx(create_connection_tx, port);

        thread::sleep(time::Duration::from_secs(3));

        (ckb_run, ckb_miner)
    }

    fn fetch_ibc_connections(port: u32) -> IbcConnections {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let url = Url::from_str(&format!("http://127.0.0.1:{}", port)).unwrap();
        let client = RpcClient::new(&url, &url);
        let mut loop_count = 0;
        loop {
            let search_connection_cell = client.fetch_live_cells(
                SearchKey {
                    script: Script::new_builder()
                        .code_hash(CONNECTION_CODE_HASH.pack())
                        .args("".pack()) // todo
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
                let (_, ibc_connection) = extract_connections_from_tx(tx).unwrap();
                return ibc_connection;
            } else {
                if loop_count > 10 {
                    panic!("connection cell cannot find on port {port}");
                }
                loop_count += 1;
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    }

    fn fetch_ibc_channel_cell(port: u32, port_id: [u8; 32]) -> IbcChannel {
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
                    panic!("channel cell cannot find on port {port}");
                }
                loop_count += 1;
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    }
}
