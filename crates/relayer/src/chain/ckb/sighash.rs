// Reference implementation:
// https://github.com/cryptape/kabletop-ckb-sdk/blob/master/src/ckb/transaction/genesis.rs

#[cfg(not(test))]
pub mod no_test {
    use super::super::prelude::CkbReader;
    use crate::error::Error;
    use ckb_jsonrpc_types::TransactionView;
    use ckb_sdk::NetworkType;
    use ckb_types::{
        core::DepType,
        packed::{CellDep, OutPoint},
        prelude::*,
    };
    use tokio::sync::OnceCell;

    const SIGHASH_GROUP_OUTPUT: (usize, usize) = (1, 0);

    static SIGHASH_CELLDEP: OnceCell<CellDep> = OnceCell::const_new();

    pub fn get_secp256k1_celldep(_network_type: NetworkType) -> CellDep {
        SIGHASH_CELLDEP
            .get()
            .expect("uninitialized sighash celldep")
            .clone()
    }

    pub async fn init_sighash_celldep(
        rpc_client: &impl CkbReader,
    ) -> Result<&'static CellDep, Error> {
        SIGHASH_CELLDEP
            .get_or_try_init(|| async {
                let block = rpc_client
                    .get_block_by_number(0.into())
                    .await
                    .map_err(|e| {
                        Error::rpc_response(format!("failed to get genesis block: {e}"))
                    })?;
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
}

#[cfg(test)]
mod test {
    use ckb_sdk::NetworkType;
    use ckb_types::{
        core::DepType,
        h256,
        packed::{CellDep, OutPoint},
        prelude::*,
    };

    pub fn get_secp256k1_celldep(network_type: NetworkType) -> CellDep {
        let celldep = CellDep::new_builder()
            .dep_type(DepType::DepGroup.into())
            .build();
        match network_type {
            NetworkType::Mainnet => celldep
                .as_builder()
                .out_point(
                    OutPoint::new_builder()
                        .tx_hash(
                            h256!("0x71a7ba8fc96349fea0ed3a5c47992e3b4084b031a42264a018e0072e8172e46c")
                                .pack(),
                        )
                        .index(0u32.pack())
                        .build(),
                )
                .build(),
            NetworkType::Testnet => celldep
                .as_builder()
                .out_point(
                    OutPoint::new_builder()
                        .tx_hash(
                            h256!("0xf8de3bb47d055cdf460d93a2a6e1b05f7432f9777c8c474abf4eec1d4aee5d37")
                                .pack(),
                        )
                        .index(0u32.pack())
                        .build(),
                )
                .build(),
            NetworkType::Dev => celldep
                .as_builder()
                .out_point(
                    OutPoint::new_builder()
                        .tx_hash(
                            h256!("0x29ed5663501cd171513155f8939ad2c9ffeb92aa4879d39cde987f8eb6274407")
                                .pack(),
                        )
                        .index(0u32.pack())
                        .build(),
                )
                .build(),
            _ => celldep,
        }
    }
}

#[cfg(not(test))]
pub use no_test::*;
#[cfg(test)]
pub use test::*;
