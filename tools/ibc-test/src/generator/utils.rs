use std::{str::FromStr, sync::Mutex};

use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::{
    core::TransactionView,
    core::{DepType, ScriptHashType},
    h256,
    packed::{CellDep, OutPoint, Script},
    prelude::{Builder, Entity, Pack, Unpack},
    H256,
};
use secp256k1::{Secp256k1, SecretKey};
use tiny_keccak::{Hasher, Keccak};

pub fn keccak256(slice: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(slice);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    output
}

pub fn get_lock_script(private: &str) -> (Script, SecretKey, Address) {
    let secret_key = SecretKey::from_str(private).unwrap();
    let public_key = secret_key.public_key(&Secp256k1::signing_only());
    let address_payload = AddressPayload::from_pubkey(&public_key);
    let address = Address::new(NetworkType::Dev, address_payload, true);
    let lock_script = Script::from(&address);
    (lock_script, secret_key, address)
}

pub fn calc_script_hash(code_hash: &H256, args: &[u8]) -> H256 {
    Script::new_builder()
        .code_hash(code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(args.pack())
        .build()
        .calc_script_hash()
        .unpack()
}

pub fn get_secp256k1_cell_dep() -> CellDep {
    CellDep::new_builder()
        .dep_type(DepType::DepGroup.into())
        .out_point(
            OutPoint::new_builder()
                .tx_hash(
                    h256!("0x29ed5663501cd171513155f8939ad2c9ffeb92aa4879d39cde987f8eb6274407")
                        .pack(),
                )
                .index(0u32.pack())
                .build(),
        )
        .build()
}
static RPC_ID: Mutex<u32> = Mutex::new(1);

pub fn wrap_rpc_request_and_save(tx: TransactionView, path: &str) -> H256 {
    let tx = ckb_jsonrpc_types::TransactionView::from(tx);
    let tx_content = serde_json::to_string(&tx.inner).unwrap();
    let id = RPC_ID.lock().unwrap();
    let rpc = format!(
        r#"{{"id": {}, "jsonrpc": "2.0", "method": "send_transaction", "params": [{}]}}"#,
        id, tx_content
    );
    std::fs::write(path, rpc).unwrap();
    drop(id);
    *RPC_ID.lock().unwrap() += 1;
    tx.hash
}
