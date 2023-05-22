// Reference implementation:
// https://github.com/cryptape/kabletop-ckb-sdk/blob/master/src/ckb/transaction/genesis.rs

use super::prelude::CkbReader;
use crate::error::Error;
use ckb_jsonrpc_types::TransactionView;
use ckb_types::{
    core::DepType,
    packed::{CellDep, OutPoint},
    prelude::*,
};
use tokio::sync::OnceCell;

const SIGHASH_GROUP_OUTPUT: (usize, usize) = (1, 0);

pub static SIGHASH_CELLDEP: OnceCell<CellDep> = OnceCell::const_new();

pub fn get_secp256k1_celldep() -> &'static CellDep {
    SIGHASH_CELLDEP
        .get()
        .expect("uninitialized sighash celldep")
}

pub async fn init_sighash_celldep(rpc_client: &impl CkbReader) -> Result<&'static CellDep, Error> {
    SIGHASH_CELLDEP
        .get_or_try_init(|| async {
            let block = rpc_client
                .get_block_by_number(0.into())
                .await
                .map_err(|e| Error::rpc_response(format!("failed to get genesis block: {e}")))?;
            let sighash_group_tx = block
                .transactions
                .get(SIGHASH_GROUP_OUTPUT.0)
                .expect("no sighash group transaction found in genesis");

            let celldep = build_celldep(sighash_group_tx, SIGHASH_GROUP_OUTPUT.1 as u32);
            tracing::info!("sighash celldep is initialized to: {celldep}");
            Ok(celldep)
        })
        .await
}

fn build_celldep(tx: &TransactionView, tx_index: u32) -> CellDep {
    let outpoint = OutPoint::new_builder()
        .tx_hash(tx.hash.pack())
        .index(tx_index.pack())
        .build();
    CellDep::new_builder()
        .out_point(outpoint)
        .dep_type(DepType::DepGroup.into())
        .build()
}
