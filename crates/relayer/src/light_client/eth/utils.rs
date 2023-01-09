#![allow(dead_code)]

use ibc_relayer_types::clients::ics07_eth::{
    header::Header,
    types::{AggregateSignature, FixedVector, PublicKey, SignatureBytes, SyncCommittee, H256, U4},
};
use sha2::{Digest, Sha256};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

pub fn is_current_committee_proof_valid(
    attested_header: &Header,
    current_committee: &mut SyncCommittee,
    current_committee_branch: &mut [H256],
) -> bool {
    is_proof_valid(
        attested_header,
        current_committee,
        current_committee_branch,
        5,
        22,
    )
}

pub fn calc_sync_period(slot: u64) -> u64 {
    let epoch = slot / 32;
    epoch / 256
}

pub fn is_finality_proof_valid(
    attested_header: &Header,
    finality_header: &mut Header,
    finality_branch: &[H256],
) -> bool {
    is_proof_valid(attested_header, finality_header, finality_branch, 6, 41)
}

pub fn is_next_committee_proof_valid(
    attested_header: &Header,
    next_committee: &mut SyncCommittee,
    next_committee_branch: &[H256],
) -> bool {
    is_proof_valid(
        attested_header,
        next_committee,
        next_committee_branch,
        5,
        23,
    )
}

pub fn is_proof_valid<L: TreeHash>(
    attested_header: &Header,
    leaf_object: &mut L,
    branch: &[H256],
    depth: usize,
    index: usize,
) -> bool {
    let object_hash = leaf_object.tree_hash_root();
    let state_root = &attested_header.state_root;

    let is_valid = is_valid_merkle_branch(&object_hash, branch.iter(), depth, index, state_root);
    is_valid
}

pub fn is_valid_merkle_branch<'a>(
    leaf: &H256,
    mut branch: impl Iterator<Item = &'a H256>,
    depth: usize,
    index: usize,
    root: &H256,
) -> bool {
    let mut value = *leaf;

    let mut hasher = Sha256::new();
    for i in 0..depth {
        let next_hash = match branch.next() {
            Some(h) => h,
            None => return false,
        };
        if (index / 2usize.pow(i as u32)) % 2 != 0 {
            hasher.update(next_hash);
            hasher.update(value);
        } else {
            hasher.update(value);
            hasher.update(next_hash);
        }
        let mut v = [0u8; 32];
        v.copy_from_slice(&hasher.finalize_reset());
        // value = hasher.finalize_reset().bytes();
        value = H256::from_slice(&v);
    }
    value == *root
}

#[derive(tree_hash_derive::TreeHash)]
pub struct SigningData {
    pub object_root: H256,
    pub domain: H256,
}

pub fn compute_signing_root(object_root: H256, domain: H256) -> H256 {
    let data = SigningData {
        object_root,
        domain,
    };
    data.tree_hash_root()
}

pub fn compute_domain(
    domain_type: &[u8],
    fork_version: FixedVector<u8, U4>,
    genesis_root: H256,
) -> H256 {
    let fork_data_root = compute_fork_data_root(fork_version, genesis_root);
    let start = domain_type;
    let end = &fork_data_root.as_bytes()[..28];
    let d = [start, end].concat();
    let v: [u8; 32] = d.try_into().unwrap();
    H256::from(v)
}

pub fn compute_fork_data_root(
    current_version: FixedVector<u8, U4>,
    genesis_validator_root: H256,
) -> H256 {
    let fork_data = ForkData {
        current_version,
        genesis_validator: genesis_validator_root,
    };
    fork_data.tree_hash_root()
}

pub fn is_aggregate_valid(sig: &SignatureBytes, msg: H256, pks: &[&PublicKey]) -> bool {
    let valid = AggregateSignature::deserialize(sig.as_ref())
        .ok()
        .map(|signature| signature.eth_fast_aggregate_verify(msg, pks));
    valid.unwrap_or(false)
}

#[derive(Debug, Default, TreeHash)]
struct ForkData {
    current_version: FixedVector<u8, U4>,
    genesis_validator: H256,
}
