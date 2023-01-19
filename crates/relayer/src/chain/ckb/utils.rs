use eth2_types::EthSpec;
use eth_light_client_in_ckb_verification::mmr;
use eth_light_client_in_ckb_verification::types::{
    core::{Client as EthLcClient, Header as EthLcHeader},
    packed::{self, Client as PackedClient, ProofUpdate as PackedProofUpdate},
    prelude::*,
};
use ibc_relayer_storage::{
    error::Error as StorageError,
    prelude::{StorageAsMMRStore, StorageReader, StorageWriter},
    Slot,
};
use ibc_relayer_types::clients::ics07_eth::types::{Header as EthHeader, Update as EthUpdate};
use tendermint_light_client::errors::Error as LightClientError;

use crate::error::Error;

pub fn get_verified_packed_client_and_proof_update<S, E>(
    chain_id: &String,
    header_updates: Vec<EthUpdate>,
    storage: &S,
    onchain_packed_client_opt: Option<PackedClient>,
) -> Result<(Option<Slot>, PackedClient, PackedProofUpdate), Error>
where
    S: StorageReader<E> + StorageWriter<E> + StorageAsMMRStore<E>,
    E: EthSpec,
{
    let mut prev_tip_slot = None;

    if header_updates.is_empty() {
        return Err(Error::empty_upgraded_client_state());
    }
    let start_slot = header_updates[0].finalized_header.slot;
    for (i, item) in header_updates.iter().enumerate() {
        if item.finalized_header.slot != i as u64 + start_slot {
            return Err(Error::send_tx("uncontinuous header slot".to_owned()));
        }
    }

    // let mut is_mmr_empty = true;
    // Check the tip in storage and the tip in the client cell are the same.
    if let Some(stored_tip_slot) = storage.get_tip_beacon_header_slot()? {
        if start_slot != stored_tip_slot + 1 {
            let height = (stored_tip_slot + 1).try_into().expect("slot too big");
            return Err(Error::light_client_verification(
                chain_id.to_string(),
                LightClientError::missing_last_block_id(height),
            ));
        }
        // is_mmr_empty = false;
    }

    // if is_mmr_empty != onchain_packed_client_opt.is_none() {
    //     return Err(Error::send_tx(
    //         "storage and the chain state is not same".to_owned(),
    //     ));
    // }

    if let Some(ref client) = onchain_packed_client_opt {
        let onchain_tip_slot: u64 = client.maximal_slot().unpack();
        if start_slot != onchain_tip_slot + 1 {
            let height = (onchain_tip_slot + 1).try_into().expect("slot too big");
            return Err(Error::light_client_verification(
                chain_id.to_string(),
                LightClientError::missing_last_block_id(height),
            ));
        }
        prev_tip_slot = Some(onchain_tip_slot);
    }

    let finalized_headers = header_updates
        .iter()
        .map(|update| {
            let EthHeader {
                slot,
                proposer_index,
                parent_root,
                state_root,
                body_root,
            } = update.finalized_header.clone();
            let header = EthLcHeader {
                slot,
                proposer_index,
                parent_root,
                state_root,
                body_root,
            };
            header.calc_cache()
        })
        .collect::<Vec<_>>();

    let minimal_slot = storage.get_base_beacon_header_slot()?.unwrap_or(start_slot);
    let last_finalized_header = &finalized_headers[finalized_headers.len() - 1];
    let maximal_slot = last_finalized_header.inner.slot;

    // Saves all header digests into storage for MMR.
    {
        let mut finalized_headers_iter = finalized_headers.iter();

        let mut last_slot = if storage.is_initialized()? {
            start_slot - 1
        } else {
            let first = finalized_headers_iter.next().expect("checked");
            storage.initialize_with(first.inner.slot, first.digest())?;
            storage.put_tip_beacon_header_slot(first.inner.slot)?;
            first.inner.slot
        };

        let mut mmr = storage.chain_root_mmr(last_slot)?;

        for header in finalized_headers_iter {
            last_slot = header.inner.slot;
            mmr.push(header.digest()).map_err(StorageError::from)?;
        }
        mmr.commit().map_err(StorageError::from)?;

        storage.put_tip_beacon_header_slot(last_slot)?;
    };

    // Gets the new root and a proof for all new headers.
    let (packed_headers_mmr_root, packed_headers_mmr_proof) = {
        let positions = (start_slot..=maximal_slot)
            .into_iter()
            .map(|slot| mmr::lib::leaf_index_to_pos(slot - minimal_slot))
            .collect::<Vec<_>>();

        let mmr = storage.chain_root_mmr(maximal_slot)?;

        let headers_mmr_root = mmr.get_root().map_err(StorageError::from)?;
        let headers_mmr_proof_items = mmr
            .gen_proof(positions)
            .map_err(StorageError::from)?
            .proof_items()
            .iter()
            .map(Clone::clone)
            .collect::<Vec<_>>();
        let headers_mmr_proof = packed::MmrProof::new_builder()
            .set(headers_mmr_proof_items)
            .build();

        (headers_mmr_root, headers_mmr_proof)
    };

    // Build the packed proof update.
    let packed_proof_update = {
        let updates_items = finalized_headers
            .iter()
            .map(|header| {
                packed::FinalityUpdate::new_builder()
                    .finalized_header(header.inner.pack())
                    .build()
            })
            .collect::<Vec<_>>();
        let updates = packed::FinalityUpdateVec::new_builder()
            .set(updates_items)
            .build();
        packed::ProofUpdate::new_builder()
            .new_headers_mmr_root(packed_headers_mmr_root)
            .new_headers_mmr_proof(packed_headers_mmr_proof)
            .updates(updates)
            .build()
    };

    // Invoke verification from core::Client on packed_proof_update
    let client = if let Some(client) = onchain_packed_client_opt {
        client
            .unpack()
            .try_apply_packed_proof_update(packed_proof_update.as_reader())
            .map_err(|_| Error::send_tx("failed to update header".to_owned()))?
    } else {
        EthLcClient::new_from_packed_proof_update(packed_proof_update.as_reader())
            .map_err(|_| Error::send_tx("failed to create header".to_owned()))?
    };

    Ok((prev_tip_slot, client.pack(), packed_proof_update))
}

#[cfg(test)]
mod tests {
    use super::get_verified_packed_client_and_proof_update;
    use super::EthHeader;
    use super::EthUpdate;
    use eth2_types::MainnetEthSpec;
    use eyre::Result;
    use ibc_relayer_storage::Storage;
    use std::fs;
    use tempfile::TempDir;

    fn load_updates_from_file(path: &str) -> Result<Vec<EthUpdate>> {
        let headers_json = fs::read_to_string(path)?;
        let headers: Vec<EthHeader> = serde_json::from_str(&headers_json)?;
        Ok(headers
            .into_iter()
            .map(EthUpdate::from_finalized_header)
            .collect::<Vec<_>>())
    }

    #[test]
    fn test_get_verified_packed_client_and_proof_update() {
        let path = TempDir::new().unwrap();
        let storage: Storage<MainnetEthSpec> = Storage::new(path).unwrap();
        let chain_id = "chain_id".to_string();

        // Testing updates for creation of light_client
        let updates_part_1 =
            load_updates_from_file("src/testdata/test_update_eth_client/headers-part-1.json")
                .expect("part_1");
        let (_, packed_client, _) =
            get_verified_packed_client_and_proof_update(&chain_id, updates_part_1, &storage, None)
                .expect("verify part_1");

        // Testing updates for update of light_client
        let updates_part_2 =
            load_updates_from_file("src/testdata/test_update_eth_client/headers-part-2.json")
                .expect("part_2");
        get_verified_packed_client_and_proof_update(
            &chain_id,
            updates_part_2,
            &storage,
            Some(packed_client),
        )
        .expect("verify part_2");
    }
}
