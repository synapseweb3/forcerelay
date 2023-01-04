use eth2_types::EthSpec;
use eth_light_client_in_ckb_verification::types::{packed, prelude::*};

use crate::{
    error::{Error, Result},
    prelude::StorageReader,
    schemas::{columns, keys},
    Slot, Storage,
};

impl<S> StorageReader<S> for Storage<S>
where
    S: EthSpec,
{
    fn get_base_beacon_header_slot(&self) -> Result<Option<Slot>> {
        let slot_opt = *self
            .cache
            .base_beacon_header_slot
            .read()
            .map_err(Error::storage)?;
        if let Some(slot) = slot_opt {
            Ok(Some(slot))
        } else {
            let slot_opt = self
                .get(keys::BASE_BEACON_HEADER_SLOT)?
                .map(|raw| packed::Uint64Reader::from_slice(&raw).map(|reader| reader.unpack()))
                .transpose()?;
            if let Some(slot) = slot_opt {
                *self
                    .cache
                    .base_beacon_header_slot
                    .write()
                    .map_err(Error::storage)? = Some(slot);
            }
            Ok(slot_opt)
        }
    }

    fn get_tip_beacon_header_slot(&self) -> Result<Option<Slot>> {
        self.get(keys::TIP_BEACON_HEADER_SLOT)?
            .map(|raw| packed::Uint64Reader::from_slice(&raw).map(|reader| reader.unpack()))
            .transpose()
            .map_err(Into::into)
    }

    fn get_beacon_header_digest(&self, position: u64) -> Result<Option<packed::HeaderDigest>> {
        let key: packed::Uint64 = position.pack();
        self.get_cf(columns::COLUMN_BEACON_HEADER_MMR, key.as_slice())?
            .map(|raw| {
                packed::HeaderDigestReader::from_slice(&raw)
                    .map(|reader| reader.to_entity())
                    .map_err(Into::into)
            })
            .transpose()
    }
}
