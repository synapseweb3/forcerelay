// TODO: refactor ckb and ckb4ibc rpc client
#[cfg(test)]
mod consts;
#[cfg(test)]
mod generator;
#[cfg(test)]
mod rpc_client;

#[cfg(test)]
mod tests {
    use super::consts::{CHANNEL_CODE_HASH, CLIENT_TYPE_ARGS, CONNECTION_CODE_HASH};
    use super::rpc_client::RpcClient;

    use ckb_hash::blake2b_256;
    use ckb_ics_axon::handler::{IbcChannel, IbcConnections};
    use ckb_ics_axon::object::State;
    use ckb_ics_axon::ChannelArgs;
    use ckb_jsonrpc_types::{Deserialize, Status};
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

    const PORT_A: u32 = 8114;
    const PORT_B: u32 = 8214;

    #[derive(Deserialize)]
    struct RpcResponse {
        pub result: H256,
    }

    impl std::fmt::Display for RpcResponse {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_fmt(format_args!("{}", self.result))
        }
    }

    #[test]
    fn test_config() {
        let path = "config.toml";
        relayer::config::load(path).unwrap();
    }

    #[ignore]
    #[test]
    fn integration_test() {
        prepare_ckb_chain("ckb-dev-a", PORT_A);
        prepare_ckb_chain("ckb-dev-b", PORT_B);

        run_create_connection(1);
        run_create_connection(2);

        run_create_channel(1);
        run_create_channel(2);
    }

    fn check_channel(channel: &IbcChannel) -> bool {
        channel.state == State::Open
    }

    fn check_ibc_connection(connection: IbcConnections, count: u32) -> bool {
        if connection.connections.len() != count as usize {
            return false;
        }
        let connection = connection.connections.into_iter().next().unwrap();
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

    fn prepare_ckb_chain(ckb_path: &str, port: u32) -> (Child, Child) {
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

        let ckb_run = Command::new("ckb")
            .arg("run")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        thread::sleep(Duration::from_secs(3));

        let ckb_miner = Command::new("ckb")
            .arg("miner")
            .current_dir(&working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

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

        (ckb_run, ckb_miner)
    }

    fn run_create_connection(count: u32) {
        println!("\n========== Create Connection No.{count} ==========\n");

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

        let a_connection = fetch_ibc_connections(PORT_A);
        println!("a_connection: {a_connection:?}");
        let b_connection = fetch_ibc_connections(PORT_B);
        println!("b_connection: {b_connection:?}");
        if !check_ibc_connection(a_connection, count) || !check_ibc_connection(b_connection, count)
        {
            panic!("create connection failed");
        }
    }

    fn run_create_channel(count: u32) {
        println!("\n========== Create Channel No.{count} ==========\n");

        let user_a_private_key = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let user_a_public_key = user_a_private_key.public_key(&Secp256k1::new()).serialize();
        let port_id_a = H256::from(blake2b_256(&user_a_public_key[..]));

        let user_b_private_key = SecretKey::from_slice(&[2u8; 32]).unwrap();
        let user_b_public_key = user_b_private_key.public_key(&Secp256k1::new()).serialize();
        let port_id_b = H256::from(blake2b_256(&user_b_public_key[..]));

        let connection_id_a = format!(
            "{}-connection-{}",
            &CLIENT_TYPE_ARGS.to_string()[..6],
            count - 1
        );
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
            .arg(port_id_a.to_string())
            .arg("--b-port")
            .arg(port_id_b.to_string())
            .arg("--a-chain")
            .arg("ckb4ibc-0")
            .arg("--a-connection")
            .arg(connection_id_a)
            .current_dir("../../")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();

        let a_channel = fetch_ibc_channel_cell(PORT_A, port_id_a.into());
        println!("a_channel: {a_channel:?}");
        let b_channel = fetch_ibc_channel_cell(PORT_B, port_id_b.into());
        println!("b_channel: {b_channel:?}");
        if !check_channel(&a_channel) || !check_channel(&b_channel) {
            panic!("create channel failed")
        }
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
                    panic!("channel cell cannot find on port {port} for {loop_count}s");
                }
                loop_count += 1;
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    }
}
