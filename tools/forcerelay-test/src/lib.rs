#[cfg(test)]
mod tests {
    use ckb_sdk::constants::TYPE_ID_CODE_HASH;
    use ckb_sdk::rpc::ckb_indexer::ScriptSearchMode;
    use ckb_sdk::{
        rpc::ckb_light_client::{Cell, Order, ScriptType, SearchKey},
        IndexerRpcClient,
    };
    use ckb_types::packed;
    use ckb_types::{core::ScriptHashType, prelude::*};
    use std::io::BufRead;
    use std::process::Command;
    use std::process::Stdio;
    use std::thread;
    use std::time::Duration;

    use eth_light_client_in_ckb_verification::types::packed::{
        Client, ClientInfo, ClientInfoReader, ClientReader,
    };
    use eth_light_client_in_ckb_verification::types::prelude::Unpack as LcUnpack;

    const MULTI_CLIENT_CONTRACT_TYPE_ID: &str =
        "f49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781";
    // const ALWAYS_SUCCESS_CONTRACT_TYPE_ID: &str = "fbe09e8ff3e5f3d0fab7cc7431feed2131846184d356a9626639f55e7f471846";

    #[test]
    fn test_config() {
        use relayer::config::load;

        let path = "config.toml";
        load(path).unwrap();
    }

    fn sleep(secs: u64) {
        thread::sleep(Duration::from_secs(secs));
    }

    #[ignore]
    #[test]
    fn integration_test() {
        let ckb_path = "ckb-dev";
        let mut working_dir = std::env::current_dir().unwrap();
        working_dir.push(ckb_path);

        let _ = std::fs::remove_dir_all(ckb_path);
        std::fs::create_dir(ckb_path).unwrap();

        let build_relayer_res = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("ibc-relayer-cli")
            .current_dir("../../")
            .status()
            .unwrap();
        assert!(build_relayer_res.success());

        let (mut ckb_run, mut ckb_miner) = {
            println!("run ckb init");
            Command::new("ckb")
                .arg("init")
                .arg("--chain")
                .arg("dev")
                .current_dir(&working_dir)
                .spawn()
                .unwrap();

            thread::sleep(Duration::from_secs(5));

            std::fs::copy("ckb/ckb.toml", format!("{}/ckb.toml", ckb_path)).unwrap();
            std::fs::copy("ckb/dev.toml", format!("{}/specs/dev.toml", ckb_path)).unwrap();
            std::fs::copy(
                "ckb/deploy-contracts-info.json",
                format!("{}/deploy-contracts-info.json", ckb_path),
            )
            .unwrap();

            println!("run ckb");
            let ckb_run = Command::new("ckb")
                .arg("run")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .current_dir(&working_dir)
                .spawn()
                .unwrap();

            thread::sleep(Duration::from_secs(1));

            println!("run ckb miner");
            let ckb_miner = Command::new("ckb")
                .arg("miner")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .current_dir(&working_dir)
                .spawn()
                .unwrap();

            thread::sleep(Duration::from_secs(10));

            // ckb-cli \
            //     deploy apply-txs \
            //     --migration-dir ./migrations \
            //     --info-file deploy-contracts-info.json

            // Deploy multi-client and always-success contracts
            println!("Deploy multi-client and always-success contracts");
            std::fs::create_dir(working_dir.join("migrations")).unwrap();
            let res = Command::new("ckb-cli")
                .arg("deploy")
                .arg("apply-txs")
                .arg("--migration-dir")
                .arg("migrations")
                .arg("--info-file")
                .arg("deploy-contracts-info.json")
                .current_dir(&working_dir)
                .status()
                .unwrap();
            assert!(res.success());

            sleep(60);

            (ckb_run, ckb_miner)
        };

        println!("run eth mock server");
        let mut running_eth_server = Command::new("python3")
            .arg("eth_mock/server.py")
            .spawn()
            .unwrap();

        // Waiting for the finality of the ckb contracts
        sleep(10);

        // RUST_LOG=info ./forcerelay --config ../config.toml eth-ckb --ethereum-chain-id ibc-eth-0 --ckb-chain-id ibc-ckb-0
        println!("run relayer");
        std::fs::copy(
            "../../target/debug/forcerelay",
            format!("{}/forcerelay", ckb_path),
        )
        .unwrap();
        let mut running_relayer = Command::new(working_dir.join("forcerelay"))
            .arg("--config")
            .arg("../config.toml")
            .arg("eth-ckb")
            .arg("--ethereum-chain-id")
            .arg("ibc-eth-0")
            .arg("--ckb-chain-id")
            .arg("ibc-ckb-0")
            .current_dir(&working_dir)
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        println!("get type id");
        let type_id = {
            // Some log are printed to stderr, some are printed to stdout.
            // For type_id, it is printed to stderr.
            let stderr = running_relayer.stderr.take().unwrap();
            let reader = std::io::BufReader::new(stderr);

            // Only take the first 50 lines to avoid waiting indefinitely.
            let mut lines = reader.lines().take(50);
            let type_id = lines
                .find_map(|line| {
                    let line = line.expect("read line failed");
                    println!("{}", line);
                    let begin_str = "worker.forcerelay: new type_id: ";
                    if let Some(pos) = line.find(begin_str) {
                        let type_id = line[pos + begin_str.len()..].trim();
                        Some(type_id.to_string())
                    } else {
                        None
                    }
                })
                .expect("cannot find type_id");
            thread::spawn(|| {
                for line in lines {
                    let line = line.expect("read line failed");
                    println!("{}", line);
                }
            });
            hex::decode(&type_id).expect("decode type_id failed")
        };

        sleep(80);

        let indexer_rpc = IndexerRpcClient::new("http://127.0.0.1:8114");
        let type_script = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(Pack::pack(
                hex::decode(MULTI_CLIENT_CONTRACT_TYPE_ID)
                    .unwrap()
                    .as_slice(),
            ))
            .build();
        let code_hash = type_script.calc_script_hash();

        let cells_count = 4u8;
        let type_args = [&type_id[..], &[cells_count]].concat();
        let search_key = SearchKey {
            script: packed::Script::new_builder()
                .code_hash(code_hash)
                .hash_type(ScriptHashType::Type.into())
                .args(Pack::pack(type_args.as_slice()))
                .build()
                .into(),
            script_type: ScriptType::Type,
            filter: None,
            with_data: None,
            group_by_transaction: None,
            script_search_mode: Some(ScriptSearchMode::Exact),
        };

        let mut retry_time = 0;

        let cells: Vec<Cell> = loop {
            if retry_time > 5 {
                println!("cannot find the light client cell, exiting");
                let _ = running_relayer.kill();
                let _ = ckb_miner.kill();
                let _ = ckb_run.kill();
                let _ = running_eth_server.kill();
                let _ = std::fs::remove_dir_all(ckb_path);
                panic!("cannot find the light client cell")
            };
            println!("searching for cells...");
            let cells = indexer_rpc.get_cells(search_key.clone(), Order::Asc, 4.into(), None);
            if let Err(e) = cells {
                eprintln!("{:?}", e);
                sleep(10);
                retry_time += 1;
                continue;
            }
            let fetched_cells = cells.unwrap().objects;
            if fetched_cells.len() == 4 {
                println!("cells found!");
                break fetched_cells;
            }
            sleep(10);
            retry_time += 1;
        };

        let mut clients: Vec<Client> = vec![];
        let mut client_info_opt: Option<ClientInfo> = None;
        for cell in cells {
            let data = cell.output_data.unwrap().into_bytes();
            if ClientReader::verify(&data[..], false).is_ok() {
                clients.push(Client::new_unchecked(data))
            } else if ClientInfoReader::verify(&data[..], false).is_ok() {
                let prev = client_info_opt.replace(ClientInfo::new_unchecked(data));
                if prev.is_some() {
                    panic!(
                        "multi client cell has more than one client info:\nfirst:\n{:?}\nsecond:\n{:?}",
                        prev.unwrap(),
                        client_info_opt.unwrap(),
                    );
                }
            } else {
                panic!("multi client cell has invalid data: {:?}", data);
            }
        }
        let Some(client_info) = client_info_opt else {
            panic!("on-chain data corrupted: client info cell not found");
        };

        let latest_client = {
            let last_id = client_info.last_id();
            clients
                .into_iter()
                .find(|client| client.id() == last_id)
                .expect("cannot find the latest client")
        };
        let min_slot = LcUnpack::unpack(&latest_client.minimal_slot());
        let max_slot = LcUnpack::unpack(&latest_client.maximal_slot());
        let tip_header_root = latest_client.tip_valid_header_root();
        assert_eq!(min_slot, 6682656);
        assert_eq!(max_slot, 6682719);
        assert_eq!(
            format!("{:#x}", tip_header_root),
            "0x3fcc91f6a81bcdcee013cb99a1e64c94123ddc8ed7151190994ab5b2b0981e3b"
        );

        let _ = running_relayer.kill();
        let _ = ckb_miner.kill();
        let _ = ckb_run.kill();
        let _ = running_eth_server.kill();
        let _ = std::fs::remove_dir_all(ckb_path);
    }
}
