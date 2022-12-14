use crate::core::ics02_client::error::Error as Ics02Error;
use ethereum_types::H256;
use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;
use serde_derive::{Deserialize, Serialize};
use ssz_types::typenum::{U48, U512, U96};
use ssz_types::{BitVector, FixedVector};
use tree_hash_derive::TreeHash;

use crate::prelude::*;

pub type BLSPubKey = FixedVector<u8, U48>;
pub type SignatureBytes = FixedVector<u8, U96>;

#[derive(Clone, PartialEq, Deserialize, Serialize, TreeHash, Default, Debug)]
pub struct Header {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: H256,
    pub state_root: H256,
    pub body_root: H256,
}

#[derive(Clone, PartialEq, TreeHash, Deserialize, Serialize, Default, Debug)]
pub struct SyncCommittee {
    pub pubkeys: FixedVector<BLSPubKey, U512>,
    pub aggregate_pubkey: BLSPubKey,
}

#[derive(Clone, PartialEq, TreeHash, Deserialize, Serialize, Default, Debug)]
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

impl crate::core::ics02_client::header::Header for Update {
    fn client_type(&self) -> crate::core::ics02_client::client_type::ClientType {
        todo!()
    }

    fn height(&self) -> crate::Height {
        todo!()
    }

    fn timestamp(&self) -> crate::timestamp::Timestamp {
        todo!()
    }
}

impl Protobuf<Any> for Update {}

impl TryFrom<Any> for Update {
    type Error = Ics02Error;

    fn try_from(_value: Any) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl From<Update> for Any {
    fn from(_header: Update) -> Self {
        todo!()
    }
}
