use crate::prelude::*;
use serde_derive::{Deserialize, Serialize};
use thiserror::Error;

pub use ethereum_types::H256;
pub use tree_hash::TreeHash;

pub use bls::{AggregateSignature, PublicKey, PublicKeyBytes};
pub use ssz_types::typenum::{U20, U256, U4, U48, U512, U96};
pub use ssz_types::{BitVector, FixedVector};

pub use super::header::Header;

pub type Address = FixedVector<u8, U20>;
pub type LogsBloom = FixedVector<u8, U256>;
pub type BLSPubKey = FixedVector<u8, U48>;
pub type SignatureBytes = FixedVector<u8, U96>;

#[derive(serde::Deserialize, Debug)]
pub struct Bootstrap {
    pub header: Header,
    pub current_sync_committee: SyncCommittee,
    pub current_sync_committee_branch: Vec<H256>,
}

#[derive(serde::Deserialize, Debug)]
pub struct FinalityUpdate {
    pub attested_header: Header,
    pub finalized_header: Header,
    pub finality_branch: Vec<H256>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: u64,
}

pub struct GenericUpdate {
    pub attested_header: Header,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: u64,
    pub next_sync_committee: Option<SyncCommittee>,
    pub next_sync_committee_branch: Option<Vec<H256>>,
    pub finalized_header: Option<Header>,
    pub finality_branch: Option<Vec<H256>>,
}

#[derive(Clone, PartialEq, tree_hash_derive::TreeHash, Deserialize, Serialize, Default, Debug)]
pub struct SyncCommittee {
    pub pubkeys: FixedVector<BLSPubKey, U512>,
    pub aggregate_pubkey: BLSPubKey,
}

#[derive(Clone, PartialEq, Deserialize, Serialize, Default, Debug)]
pub struct SyncAggregate {
    pub sync_committee_bits: BitVector<U512>,
    pub sync_committee_signature: SignatureBytes,
}

#[derive(Clone, PartialEq, Deserialize, Serialize, Default, Debug)]
pub struct Update {
    pub attested_header: Header,
    pub next_sync_committee: SyncCommittee,
    pub next_sync_committee_branch: Vec<H256>,
    pub finalized_header: Header,
    pub finality_branch: Vec<H256>,
    pub sync_aggregate: SyncAggregate,
    pub signature_slot: u64,
}

impl From<&Update> for GenericUpdate {
    fn from(update: &Update) -> Self {
        GenericUpdate {
            attested_header: update.attested_header.clone(),
            sync_aggregate: update.sync_aggregate.clone(),
            signature_slot: update.signature_slot.clone(),
            next_sync_committee: Some(update.next_sync_committee.clone()),
            next_sync_committee_branch: Some(update.next_sync_committee_branch.clone()),
            finalized_header: Some(update.finalized_header.clone()),
            finality_branch: Some(update.finality_branch.clone()),
        }
    }
}

impl From<&FinalityUpdate> for GenericUpdate {
    fn from(value: &FinalityUpdate) -> Self {
        GenericUpdate {
            attested_header: value.attested_header.clone(),
            sync_aggregate: value.sync_aggregate.clone(),
            signature_slot: value.signature_slot.clone(),
            next_sync_committee: None,
            next_sync_committee_branch: None,
            finalized_header: Some(value.finalized_header.clone()),
            finality_branch: Some(value.finality_branch.clone()),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub consensus_rpc: String,
    pub execution_rpc: String,
    pub rpc_port: Option<u16>,
    pub checkpoint: H256,
    pub max_checkpoint_age: u64,
    pub forks: Forks,
    // ChainConfig
    pub chain_id: u64,
    pub genesis_time: u64,
    pub genesis_root: H256, // Vec<u8> in helios
}

impl Config {
    pub fn fork_version(&self, slot: u64) -> FixedVector<u8, U4> {
        let epoch = slot / 32;

        if epoch >= self.forks.bellatrix.epoch {
            self.forks.bellatrix.fork_version.clone()
        } else if epoch >= self.forks.altair.epoch {
            self.forks.altair.fork_version.clone()
        } else {
            self.forks.genesis.fork_version.clone()
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Forks {
    pub genesis: Fork,
    pub altair: Fork,
    pub bellatrix: Fork,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Fork {
    pub epoch: u64,
    pub fork_version: FixedVector<u8, U4>,
}

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("insufficient participation")]
    InsufficientParticipation,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("invalid sync committee period")]
    InvalidPeriod,
    #[error("update not relevant")]
    NotRelevant,
    #[error("invalid finality proof")]
    InvalidFinalityProof,
    #[error("invalid next sync committee proof")]
    InvalidNextSyncCommitteeProof,
    #[error("invalid current sync committee proof")]
    InvalidCurrentSyncCommitteeProof,
    #[error("invalid sync committee signature")]
    InvalidSignature,
    #[error("invalid header hash found: {0}, expected: {1}")]
    InvalidHeaderHash(H256, H256),
    #[error("payload not found for slot: {0}")]
    PayloadNotFound(u64),
    #[error("checkpoint is too old")]
    CheckpointTooOld,
}
