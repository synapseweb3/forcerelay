use std::str::FromStr;

use axon_tools::types::{Block as AxonBlock, Proof as AxonProof, ValidatorExtend};
use ckb_ics_axon::proof::{
    Log as CkbLog, ObjectProof, TransactionReceipt as CkbTransactionReceipt,
};
use rlp::Encodable;

use crate::{
    chain::SEC_TO_NANO,
    client_state::{AnyClientState, IdentifiedAnyClientState},
    consensus_state::AnyConsensusState,
    error::Error,
    event::IbcEventWithHeight,
    ibc_contract::OwnableIBCHandlerEvents,
};
use ethers::{types::TransactionReceipt, types::H256, utils::rlp};
use ibc_relayer_types::{
    clients::{
        ics07_axon::{client_state::AxonClientState, consensus_state::AxonConsensusState},
        ics07_ckb::{client_state::CkbClientState, consensus_state::CkbConsensusState},
    },
    core::{ics02_client::client_type::ClientType, ics24_host::identifier::ClientId},
    timestamp::Timestamp,
    Height,
};

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

pub fn ibc_event_from_ibc_handler_event(
    height: Height,
    tx_hash: [u8; 32],
    event: OwnableIBCHandlerEvents,
) -> Result<Option<IbcEventWithHeight>, eyre::Error> {
    use crate::chain::axon::*;
    use ibc_relayer_types::core::ics02_client::events as clients;
    use ibc_relayer_types::core::ics03_connection::events as connections;
    use ibc_relayer_types::core::ics04_channel::events as channels;
    use ibc_relayer_types::core::ics04_channel::packet::Packet;

    let event: IbcEvent = match event {
        OwnableIBCHandlerEvents::AcknowledgePacketFilter(data) => {
            let contract::PacketData {
                sequence,
                source_port,
                source_channel,
                destination_port,
                destination_channel,
                data,
                timeout_height,
                timeout_timestamp,
            } = data.packet;

            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;

            let packet = Packet {
                sequence: sequence.into(),
                source_port: PortId::from_str(&source_port)?,
                source_channel: ChannelId::from_str(&source_channel)?,
                destination_port: PortId::from_str(&destination_port)?,
                destination_channel: ChannelId::from_str(&destination_channel)?,
                data: data.0.to_vec(),
                timeout_height: timeout_height.into(),
                timeout_timestamp: Timestamp::from_nanoseconds(timeout_timestamp * SEC_TO_NANO)?,
            };
            IbcEvent::AcknowledgePacket(channels::AcknowledgePacket { packet })
        }
        OwnableIBCHandlerEvents::CloseConfirmChannelFilter(data) => {
            let contract::CloseConfirmChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::CloseConfirmChannel(channels::CloseConfirm {
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                port_id: PortId::from_str(&port_id)?,
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::CloseInitChannelFilter(data) => {
            let contract::CloseInitChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::CloseInitChannel(channels::CloseInit {
                port_id: PortId::from_str(&port_id)?,
                channel_id: ChannelId::from_str(&channel_id)?,
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::CreateClientFilter(data) => {
            let contract::CreateClientFilter {
                client_id,
                client_type,
            } = data;
            IbcEvent::CreateClient(clients::CreateClient(clients::Attributes {
                client_id: ClientId::from_str(&client_id)?,
                client_type: ClientType::from_str(&client_type)?,
                consensus_height: height,
            }))
        }
        OwnableIBCHandlerEvents::OpenAckChannelFilter(data) => {
            let contract::OpenAckChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenAckChannel(channels::OpenAck {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenAckConnectionFilter(data) => {
            let contract::OpenAckConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenAckConnection(connections::OpenAck(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenConfirmChannelFilter(data) => {
            let contract::OpenConfirmChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenConfirmChannel(channels::OpenConfirm {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenConfirmConnectionFilter(data) => {
            let contract::OpenConfirmConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenConfirmConnection(connections::OpenConfirm(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenInitChannelFilter(data) => {
            let contract::OpenInitChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenInitChannel(channels::OpenInit {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenInitConnectionFilter(data) => {
            let contract::OpenInitConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenInitConnection(connections::OpenInit(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenTryChannelFilter(data) => {
            let contract::OpenTryChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenTryChannel(channels::OpenTry {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenTryConnectionFilter(data) => {
            let contract::OpenTryConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenTryConnection(connections::OpenTry(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::ReceivePacketFilter(data) => {
            let contract::ReceivePacketFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
            } = data;
            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;
            IbcEvent::ReceivePacket(channels::ReceivePacket {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.into(),
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
            })
        }
        OwnableIBCHandlerEvents::SendPacketFilter(data) => {
            let contract::SendPacketFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
            } = data;
            let timeout_height = Height::from_noncosmos_height(timeout_height.revision_height);
            IbcEvent::SendPacket(channels::SendPacket {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.into(),
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
            })
        }
        OwnableIBCHandlerEvents::UpdateClientFilter(data) => {
            let contract::UpdateClientFilter {
                client_id,
                client_message: _,
            } = data;
            let client_id = ClientId::from_str(&client_id)?;
            let client_type = ClientType::from(client_id.clone());
            IbcEvent::UpdateClient(UpdateClient {
                common: clients::Attributes {
                    client_id,
                    client_type,
                    consensus_height: height,
                },
                header: None,
            })
        }
        OwnableIBCHandlerEvents::WriteAcknowledgementFilter(data) => {
            let contract::WriteAcknowledgementFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
                acknowledgement,
            } = data;

            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;
            IbcEvent::WriteAcknowledgement(channels::WriteAcknowledgement {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.try_into()?,
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
                ack: acknowledgement.to_vec(),
            })
        }
        OwnableIBCHandlerEvents::RegisterCellEmitterFilterFilter(_)
        | OwnableIBCHandlerEvents::RemoveCellEmitterFilterFilter(_)
        | OwnableIBCHandlerEvents::OwnershipTransferredFilter(_) => return Ok(None),
    };
    Ok(Some(IbcEventWithHeight {
        event,
        height,
        tx_hash,
    }))
}

pub fn generate_debug_content(
    block: &AxonBlock,
    state_root: &H256,
    block_proof: &AxonProof,
    validators: &Vec<ValidatorExtend>,
) -> String {
    let block = serde_json::to_string_pretty(block).unwrap();
    let validators = serde_json::to_string_pretty(validators).unwrap();
    let state_root = hex::encode(state_root);
    let block_proof = serde_json::to_string_pretty(block_proof).unwrap();
    let content = format!("[block]\n{block}\n[validators]\n{validators}\n[state_root]\n{state_root}\n[block_proof]\n{block_proof}");
    content
}
