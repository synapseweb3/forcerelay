use eth2_types::EthSpec;
use eth_light_client_in_ckb_verification::{
    mmr::{self, ClientRootMMR},
    types::packed,
};

use crate::{
    error::{Error, Result},
    Slot, Storage,
};

pub trait StorageReader<S: EthSpec>: Send + Sync + Sized {
    fn get_base_beacon_header_slot(&self) -> Result<Option<Slot>>;
    fn get_tip_beacon_header_slot(&self) -> Result<Option<Slot>>;

    fn get_beacon_header_digest(&self, position: u64) -> Result<Option<packed::HeaderDigest>>;
}

pub trait StorageWriter<S: EthSpec>: Send + Sync + Sized {
    fn put_base_beacon_header_slot(&self, slot: Slot) -> Result<()>;
    fn put_tip_beacon_header_slot(&self, slot: Slot) -> Result<()>;

    fn delete_base_beacon_header_slot(&self) -> Result<()>;
    fn delete_tip_beacon_header_slot(&self) -> Result<()>;

    fn put_beacon_header_digest(&self, position: u64, digest: &packed::HeaderDigest) -> Result<()>;
}

pub trait StorageAsMMRStore<S: EthSpec>:
    mmr::lib::MMRStoreReadOps<packed::HeaderDigest>
    + mmr::lib::MMRStoreWriteOps<packed::HeaderDigest>
    + StorageReader<S>
    + StorageWriter<S>
    + Clone
{
    /// Checks if there is any data in the MMR.
    fn is_initialized(&self) -> Result<bool> {
        self.get_base_beacon_header_slot()
            .map(|inner| inner.is_some())
    }

    fn rollback_to(&self, slot_opt: Option<Slot>) -> Result<()> {
        if let Some(slot) = slot_opt {
            self.put_tip_beacon_header_slot(slot)?;
        } else {
            self.delete_base_beacon_header_slot()?;
            self.delete_tip_beacon_header_slot()?;
        }
        Ok(())
    }

    fn initialize_with(&self, slot: Slot, digest: packed::HeaderDigest) -> Result<()> {
        self.put_base_beacon_header_slot(slot)?;
        let mut mmr = ClientRootMMR::new(0, self.clone());
        mmr.push(digest)?;
        mmr.commit()?;
        Ok(())
    }

    /// Returns the chain root MMR for a provided slot.
    fn chain_root_mmr(&self, curr: Slot) -> Result<ClientRootMMR<Self>> {
        if let Some(base) = self.get_base_beacon_header_slot()? {
            let index = curr - base;
            let mmr_size = mmr::lib::leaf_index_to_mmr_size(index);
            let mmr = ClientRootMMR::new(mmr_size, self.clone());
            Ok(mmr)
        } else {
            Err(Error::data("no headers"))
        }
    }
}

impl<S: EthSpec> StorageAsMMRStore<S> for Storage<S> {}
