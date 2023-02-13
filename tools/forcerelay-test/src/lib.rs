#[cfg(test)]
mod tests {
    use ckb_jsonrpc_types::JsonBytes;
    use ckb_sdk::constants::TYPE_ID_CODE_HASH;
    use ckb_sdk::{
        rpc::ckb_light_client::{Order, ScriptType, SearchKey},
        IndexerRpcClient,
    };
    use ckb_types::packed;
    use ckb_types::{core::ScriptHashType, prelude::*};
    use std::process::Command;
    use std::thread;
    use std::time;
    use std::time::Duration;

    use eth_light_client_in_ckb_verification::types::packed::{ClientReader, Uint64};
    #[ignore]
    #[test]
    fn integration_test() {
        let ckb_path = "ckb-dev";
        let mut working_dir = std::env::current_dir().unwrap();
        working_dir.push(ckb_path);

        let _ = std::fs::remove_dir_all(ckb_path);
        std::fs::create_dir(ckb_path).unwrap();

        let (mut ckb_run, mut ckb_miner) = {
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

            let ckb_run = Command::new("ckb")
                .arg("run")
                .current_dir(&working_dir)
                .spawn()
                .unwrap();

            thread::sleep(Duration::from_secs(1));

            let ckb_miner = Command::new("ckb")
                .arg("miner")
                .current_dir(&working_dir)
                .spawn()
                .unwrap();

            thread::sleep(Duration::from_secs(10));

            let tx_rpc_file = "ckb/deploy-contracts.json";

            // deploy the contracts
            Command::new("curl")
                .arg("-H")
                .arg("content-type: application/json")
                .arg("-d")
                .arg(format!("@{}", tx_rpc_file))
                .arg("http://localhost:8114")
                .spawn()
                .unwrap();

            let three_secs = time::Duration::from_secs(3);
            thread::sleep(three_secs);

            (ckb_run, ckb_miner)
        };

        let mut running_eth_server = Command::new("python3")
            .arg("eth_mock/server.py")
            .spawn()
            .unwrap();

        // Waiting for the finality of the ckb contracts
        let ten_secs = time::Duration::from_secs(10);
        thread::sleep(ten_secs);

        let mut running_relayer = Command::new("cargo")
            .arg("run")
            .arg("--")
            .arg("--config")
            .arg("./tools/forcerelay-test/config.toml")
            .arg("forcerelay")
            .arg("--ethereum-chain-id")
            .arg("ibc-eth-0")
            .arg("--ckb-chain-id")
            .arg("ibc-ckb-0")
            .current_dir("../../")
            .spawn()
            .unwrap();

        let five_secs = time::Duration::from_secs(5);
        thread::sleep(five_secs);

        let mut client = IndexerRpcClient::new("http://127.0.0.1:8114");
        let type_script = packed::Script::new_builder()
            .code_hash(TYPE_ID_CODE_HASH.0.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(
                hex::decode("f49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781")
                    .unwrap()
                    .pack(),
            )
            .build();
        let code_hash = type_script.calc_script_hash();

        let search_key = SearchKey {
            script: packed::Script::new_builder()
                .code_hash(code_hash)
                .hash_type(ScriptHashType::Type.into())
                .build()
                .into(),
            script_type: ScriptType::Type,
            filter: None,
            with_data: None,
            group_by_transaction: None,
        };

        let mut retry_time = 0;

        let mut data: Option<JsonBytes> = None;
        while data.is_none() {
            if retry_time > 40 {
                let _ = running_relayer.kill();
                let _ = ckb_miner.kill();
                let _ = ckb_run.kill();
                let _ = running_eth_server.kill();
                let _ = std::fs::remove_dir_all(ckb_path);
                panic!("cannot find the light client cell")
            };
            let cells = client.get_cells(search_key.clone(), Order::Asc, 1u32.into(), None);
            if cells.is_err() {
                thread::sleep(time::Duration::from_secs(30));
                retry_time += 1;
                continue;
            }
            let objects = cells.unwrap().objects;
            if objects.len() == 1 {
                let cell = objects.get(0).unwrap().clone();
                data = Some(cell.output_data.unwrap());
                break;
            }
            thread::sleep(time::Duration::from_secs(30));
            retry_time += 1;
        }

        let data = data.unwrap();
        let reader = ClientReader::new_unchecked(data.as_bytes());
        ClientReader::verify(data.as_bytes(), true).unwrap();
        let min_slot = reader.minimal_slot().to_entity();
        let max_slot = reader.maximal_slot().to_entity();
        let tip_header_root = reader.tip_valid_header_root().to_entity();
        assert_eq!(uint64_to_u64(min_slot), 5787808);
        assert_eq!(uint64_to_u64(max_slot), 5787839);
        assert_eq!(
            format!("{:#x}", tip_header_root),
            "0xd5d43d373474c15856b155877994e551e40477c0a57d0b5abbdcf94904881623"
        );

        let _ = running_relayer.kill();
        let _ = ckb_miner.kill();
        let _ = ckb_run.kill();
        let _ = running_eth_server.kill();
        let _ = std::fs::remove_dir_all(ckb_path);
    }

    fn uint64_to_u64(n: Uint64) -> u64 {
        let bytes: [u8; 8] = n.as_slice().try_into().unwrap();
        u64::from_le_bytes(bytes)
    }
}
