use primitive_types::{H160, H256};
use ssz_types::{
    typenum::{U256, U32},
    FixedVector, VariableList,
};

type BytesPerLogsBloom = U256;
type MaxExtraDataBytes = U32;

#[derive(Debug, Clone, Default)]
pub struct ExecutionBlockHash(H256);

#[derive(Debug, Clone, Default)]
pub struct ExecutionPayloadHeader {
    pub parent_hash: ExecutionBlockHash,
    pub fee_receipient: H160,
    pub state_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: FixedVector<u8, BytesPerLogsBloom>,
    pub prev_randao: H256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub extra_data: VariableList<u8, MaxExtraDataBytes>,
    pub base_fee_per_gas: U256,
    pub block_hash: ExecutionBlockHash,
    pub transactions_root: H256,
}
