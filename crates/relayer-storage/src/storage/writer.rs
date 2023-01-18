use eth2_types::EthSpec;
use eth_light_client_in_ckb_verification::types::{packed, prelude::*};

use crate::{
    error::{Error, Result},
    prelude::StorageWriter,
    schemas::{columns, keys},
    Slot, Storage,
};

impl<S> StorageWriter<S> for Storage<S>
where
    S: EthSpec,
{
    fn put_base_beacon_header_slot(&self, slot: Slot) -> Result<()> {
        let value = slot.pack();
        let mut writer = self
            .cache
            .base_beacon_header_slot
            .write()
            .map_err(Error::storage)?;
        self.put(keys::BASE_BEACON_HEADER_SLOT, value.as_slice())?;
        *writer = Some(slot);
        Ok(())
    }

    fn put_tip_beacon_header_slot(&self, slot: Slot) -> Result<()> {
        let value = slot.pack();
        self.put(keys::TIP_BEACON_HEADER_SLOT, value.as_slice())
    }

    fn delete_base_beacon_header_slot(&self) -> Result<()> {
        let mut writer = self
            .cache
            .base_beacon_header_slot
            .write()
            .map_err(Error::storage)?;
        self.delete(keys::BASE_BEACON_HEADER_SLOT)?;
        *writer = None;
        Ok(())
    }

    fn delete_tip_beacon_header_slot(&self) -> Result<()> {
        self.delete(keys::TIP_BEACON_HEADER_SLOT)
    }

    fn put_beacon_header_digest(&self, position: u64, digest: &packed::HeaderDigest) -> Result<()> {
        let key: packed::Uint64 = position.pack();
        self.put_cf(
            columns::COLUMN_BEACON_HEADER_MMR,
            key.as_slice(),
            digest.as_slice(),
        )
    }
}
