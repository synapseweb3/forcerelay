use std::str::FromStr;

use axon_tools::types::{AxonBlock, Proof as AxonProof};
use ckb_ics_axon::proof::{
    Log as CkbLog, ObjectProof, TransactionReceipt as CkbTransactionReceipt,
};
use rlp::Encodable;

use crate::{
    client_state::{AnyClientState, IdentifiedAnyClientState},
    consensus_state::AnyConsensusState,
    error::Error,
};
use ethers::{types::TransactionReceipt, types::H256, utils::rlp};
use ibc_relayer_types::{
    clients::{
        ics07_axon::{client_state::AxonClientState, consensus_state::AxonConsensusState},
        ics07_ckb::{client_state::CkbClientState, consensus_state::CkbConsensusState},
    },
    core::{ics02_client::client_type::ClientType, ics24_host::identifier::ClientId},
    timestamp::Timestamp,
};

pub const SEC_TO_NANO: u64 = 1_000_000_000;

pub fn to_timestamp(seconds: u64) -> Result<Timestamp, Error> {
    Timestamp::from_nanoseconds(seconds * SEC_TO_NANO).map_err(convert_err)
}

pub fn convert_err<T: ToString>(err: T) -> Error {
    Error::other_error(err.to_string())
}

pub fn to_identified_any_client_state(
    client_state: &ethers::core::types::Bytes,
) -> Result<IdentifiedAnyClientState, Error> {
    let (client_id, client_state) = to_any_client_state(client_state)?;
    Ok(IdentifiedAnyClientState {
        client_id,
        client_state,
    })
}

// response format designed by IBC solidity: "ClientId|JSON(ClientState)"
pub fn to_any_client_state(
    response: &ethers::core::types::Bytes,
) -> Result<(ClientId, AnyClientState), Error> {
    let (client_id, client_state) = {
        let response =
            String::from_utf8(response.to_vec()).map_err(|e| Error::other_error(e.to_string()))?;
        let result = response
            .split('|')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert!(result.len() == 2);
        let client_id = ClientId::from_str(result[0].as_str())
            .map_err(|e| Error::other_error(e.to_string()))?;
        let client_state = result[1].clone();
        (client_id, client_state)
    };
    let any_client_state = match client_id.clone().into() {
        ClientType::Axon => serde_json::from_slice::<AxonClientState>(client_state.as_bytes())
            .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Axon)))?
            .into(),
        ClientType::Ckb4Ibc => serde_json::from_slice::<CkbClientState>(client_state.as_bytes())
            .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Ckb4Ibc)))?
            .into(),
        // currently, only support Axon and Ckb4Ibc
        _ => unimplemented!(),
    };
    Ok((client_id, any_client_state))
}

// response format designed by IBC solidity: "ClientId|JSON(ConsensusState)"
pub fn to_any_consensus_state(
    response: &ethers::core::types::Bytes,
) -> Result<AnyConsensusState, Error> {
    let (client_id, consensus_state) = {
        let response =
            String::from_utf8(response.to_vec()).map_err(|e| Error::other_error(e.to_string()))?;
        let result = response
            .split('|')
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert!(result.len() == 2);
        let client_id = ClientId::from_str(result[0].as_str())
            .map_err(|e| Error::other_error(e.to_string()))?;
        let consensus_state = result[1].clone();
        (client_id, consensus_state)
    };
    let any_consensus_state = match client_id.into() {
        ClientType::Axon => {
            serde_json::from_slice::<AxonConsensusState>(consensus_state.as_bytes())
                .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Axon)))?
                .into()
        }
        ClientType::Ckb4Ibc => {
            serde_json::from_slice::<CkbConsensusState>(consensus_state.as_bytes())
                .map_err(|e| Error::client_state_type(format!("{}: {e}", ClientType::Ckb4Ibc)))?
                .into()
        }
        // currently, only support Axon and Ckb4Ibc
        _ => unimplemented!(),
    };
    Ok(any_consensus_state)
}

// use ObjectProof in Ckb repo to garrantee the correctness of encode/decode of Axon proof
pub fn to_ckb_like_object_proof(
    receipt: &TransactionReceipt,
    receipt_proof: &[Vec<u8>],
    block: &AxonBlock,
    state_root: &H256,
    block_proof: &AxonProof,
) -> ObjectProof {
    let logs = receipt
        .logs
        .iter()
        .map(|log| CkbLog {
            address: log.address,
            topics: log.topics.clone(),
            data: log.data.to_vec().into(),
            block_hash: log.block_hash,
            block_number: log.block_number,
            transaction_hash: log.transaction_hash,
            transaction_index: log.transaction_index,
            log_index: log.log_index,
            transaction_log_index: log.transaction_log_index,
            log_type: log.log_type.clone(),
            removed: log.removed,
        })
        .collect();
    let receipt = CkbTransactionReceipt {
        transaction_hash: receipt.transaction_hash,
        transaction_index: receipt.transaction_index,
        block_hash: receipt.block_hash,
        block_number: receipt.block_number,
        from: receipt.from,
        to: receipt.to,
        cumulative_gas_used: receipt.cumulative_gas_used,
        gas_used: receipt.gas_used,
        contract_address: receipt.contract_address,
        logs,
        status: receipt.status,
        root: receipt.root,
        logs_bloom: receipt.logs_bloom,
        transaction_type: receipt.transaction_type,
        effective_gas_price: receipt.effective_gas_price,
    };
    let block = block.rlp_bytes().to_vec();
    let axon_proof = block_proof.rlp_bytes().to_vec();
    ObjectProof {
        receipt,
        receipt_proof: receipt_proof.to_owned(),
        block,
        state_root: *state_root,
        axon_proof,
    }
}
