use eth2_types::EthSpec;
use eth_light_client_in_ckb_verification::{
    mmr::lib::{Error as MMRError, MMRStoreReadOps, MMRStoreWriteOps, Result as MMRResult},
    types::packed,
};

use super::Storage;
use crate::prelude::*;

impl<S> MMRStoreReadOps<packed::HeaderDigest> for Storage<S>
where
    S: EthSpec,
{
    fn get_elem(&self, pos: u64) -> MMRResult<Option<packed::HeaderDigest>> {
        self.get_beacon_header_digest(pos).map_err(|err| {
            MMRError::StoreError(format!(
                "Failed to read position {} from MMR, DB error {}",
                pos, err
            ))
        })
    }
}

impl<S> MMRStoreWriteOps<packed::HeaderDigest> for Storage<S>
where
    S: EthSpec,
{
    fn append(&mut self, pos: u64, elems: Vec<packed::HeaderDigest>) -> MMRResult<()> {
        for (offset, elem) in elems.iter().enumerate() {
            let pos: u64 = pos + (offset as u64);
            self.put_beacon_header_digest(pos, elem).map_err(|err| {
                MMRError::StoreError(format!("Failed to append to MMR, DB error {}", err))
            })?;
        }
        Ok(())
    }
}
