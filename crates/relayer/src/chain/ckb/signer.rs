use ckb_hash::new_blake2b;
use ckb_types::{
    bytes::Bytes,
    core::TransactionView,
    packed::{self, Byte32, CellOutput, WitnessArgs},
    prelude::*,
};
use std::collections::HashMap;

use crate::keyring::errors::Error;
use crate::keyring::SigningKeyPair;

// sign a whole [tx] using private [key], the [extra_witnesses] is some external args which just placed into witness part
// the function just supposes two or more cells that are in one group are all close together
pub fn sign<S: SigningKeyPair + Clone>(
    tx: TransactionView,
    inputs: &[CellOutput],
    extra_witnesses: Vec<WitnessArgs>,
    signer: S,
) -> Result<TransactionView, Error> {
    #[allow(clippy::mutable_key_type)]
    let mut last_lockhashes: HashMap<Byte32, (WitnessArgs, usize, Vec<packed::Bytes>)> =
        HashMap::new();
    let mut signed_witnesses = inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let mut witness = {
                if let Some(witness) = tx.witnesses().get(i) {
                    witness
                } else {
                    Bytes::new().pack()
                }
            };
            let lockhash = input.lock().calc_script_hash();
            if let Some((_, _, group_witnesses)) = last_lockhashes.get_mut(&lockhash) {
                group_witnesses.push(witness.clone());
            } else {
                let witness_args = {
                    if witness.as_slice() == Bytes::new().pack().as_slice() {
                        WitnessArgs::default()
                    } else {
                        let witness: Bytes = witness.unpack();
                        WitnessArgs::from_slice(witness.to_vec().as_slice()).unwrap_or_default()
                    }
                };
                last_lockhashes.insert(lockhash, (witness_args, i, vec![]));
                witness = Bytes::new().pack();
            }
            witness
        })
        .collect::<Vec<_>>();
    for (_, (witness, i, group_witnesses)) in last_lockhashes {
        signed_witnesses[i] = sign_input(
            tx.hash(),
            signer.clone(),
            &witness,
            &group_witnesses,
            &extra_witnesses,
        )?;
    }
    let mut extra_witnesses = extra_witnesses
        .iter()
        .map(|witness| witness.as_bytes().pack())
        .collect::<Vec<_>>();
    signed_witnesses.append(&mut extra_witnesses);
    Ok(tx
        .as_advanced_builder()
        .set_witnesses(signed_witnesses)
        .build())
}

// sign the every single input data in [tx] and get the signed bytes
fn sign_input(
    tx_hash: Byte32,
    signer: impl SigningKeyPair,
    witness: &WitnessArgs,
    group_witnesses: &Vec<packed::Bytes>,
    extra_witnesses: &Vec<WitnessArgs>,
) -> Result<packed::Bytes, Error> {
    let mut blake2b = new_blake2b();
    blake2b.update(&tx_hash.raw_data());
    let signed_witness = witness
        .clone()
        .as_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = signed_witness.as_bytes().len() as u64;
    blake2b.update(&witness_len.to_le_bytes());
    blake2b.update(&signed_witness.as_bytes());
    for group_witness in group_witnesses {
        let witness_len = group_witness.raw_data().len() as u64;
        blake2b.update(&witness_len.to_le_bytes());
        blake2b.update(&group_witness.raw_data());
    }
    for extra_witness in extra_witnesses {
        let witness_len = extra_witness.as_bytes().len() as u64;
        blake2b.update(&witness_len.to_le_bytes());
        blake2b.update(&extra_witness.as_bytes());
    }
    let mut digest = [0u8; 32];
    blake2b.finalize(&mut digest);
    let signature = signer.sign(&digest)?;
    Ok(signed_witness
        .as_builder()
        .lock(Some(Bytes::from(signature)).pack())
        .build()
        .as_bytes()
        .pack())
}
