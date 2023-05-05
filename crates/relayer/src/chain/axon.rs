#![allow(dead_code, unused_variables, unused_imports)]
#![allow(clippy::diverging_sub_expression)]

use std::{
    collections::HashMap,
    str::FromStr,
    sync::{self, Arc},
    thread,
};

use crate::{
    account::Balance,
    chain::{axon::contract::HeightData, requests::QueryHeight},
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, filter::port, ChainConfig},
    connection::ConnectionMsgType,
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::axon::LightClient as AxonLightClient,
    misbehaviour::MisbehaviourEvidence,
    util::collate::collate,
};
use ethers::types::{Block, BlockId, Transaction, TransactionReceipt, TxHash};
use ethers_providers::Middleware;
use futures::TryFutureExt;
use ibc_proto::ibc::core::channel::v1::IdentifiedChannel;
use ibc_relayer_types::{
    applications::ics31_icq::response::CrossChainQueryResponse,
    clients::{
        ics07_axon::{
            client_state::ClientState as AxonClientState,
            consensus_state::ConsensusState as AxonConsensusState, header::Header as AxonHeader,
        },
        ics07_ckb::client_state,
    },
    core::{
        ics02_client::events::UpdateClient,
        ics03_connection::{
            self,
            connection::{self, ConnectionEnd, IdentifiedConnectionEnd},
        },
        ics04_channel::{
            self,
            channel::{self, ChannelEnd, IdentifiedChannelEnd},
            events::OpenInit,
            packet::Sequence,
        },
        ics23_commitment::{
            commitment::{CommitmentPrefix, CommitmentProofBytes},
            merkle::MerkleProof,
        },
        ics24_host::identifier::{self, ChainId, ChannelId, ClientId, ConnectionId, PortId},
    },
    proofs::Proofs,
    signer::Signer,
    Height,
};
use itertools::Itertools;
use tendermint_rpc::{endpoint::broadcast::tx_sync::Response, query};

use self::{
    contract::{OwnableIBCHandler, OwnableIBCHandlerEvents},
    monitor::AxonEventMonitor,
};

type ContractProvider = ethers_providers::Provider<ethers_providers::Ws>;
type Contract = OwnableIBCHandler<ContractProvider>;
type ContractEvents = OwnableIBCHandlerEvents;

use super::{
    client::ClientSettings,
    endpoint::{ChainEndpoint, ChainStatus, HealthCheck},
    handle::Subscription,
    requests::{
        self, CrossChainQueryRequest, IncludeProof, QueryChannelClientStateRequest,
        QueryChannelRequest, QueryChannelsRequest, QueryClientConnectionsRequest,
        QueryClientStateRequest, QueryClientStatesRequest, QueryConnectionChannelsRequest,
        QueryConnectionRequest, QueryConnectionsRequest, QueryConsensusStateHeightsRequest,
        QueryConsensusStateRequest, QueryHostConsensusStateRequest,
        QueryNextSequenceReceiveRequest, QueryPacketAcknowledgementRequest,
        QueryPacketAcknowledgementsRequest, QueryPacketCommitmentRequest,
        QueryPacketCommitmentsRequest, QueryPacketEventDataRequest, QueryPacketReceiptRequest,
        QueryTxRequest, QueryUnreceivedAcksRequest, QueryUnreceivedPacketsRequest,
        QueryUpgradedClientStateRequest, QueryUpgradedConsensusStateRequest,
    },
    tracking::TrackedMsgs,
};
use tokio::runtime::{self, Runtime as TokioRuntime};

mod contract;
mod monitor;

pub struct AxonChain {
    pub rt: Arc<TokioRuntime>,
    pub config: AxonChainConfig,
    pub light_client: AxonLightClient,
    pub tx_monitor_cmd: Option<TxMonitorCmd>,
    pub contract: Contract,
    client: ContractProvider,
    keybase: KeyRing<Secp256k1KeyPair>,
    conn_tx_hash: HashMap<(ConnectionId, connection::State), [u8; 32]>,
}

// Allow temporarily for development. Should remove when work is done.
#[allow(unreachable_code)]
impl ChainEndpoint for AxonChain {
    type LightBlock = ChainId;
    type Header = AxonHeader;
    type ConsensusState = AxonConsensusState;
    type ClientState = AxonClientState;
    type SigningKeyPair = Secp256k1KeyPair;

    fn config(&self) -> ChainConfig {
        ChainConfig::Axon(self.config.clone())
    }

    fn bootstrap(config: ChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error> {
        let config: AxonChainConfig = config.try_into()?;
        let mut light_client = AxonLightClient::from_config(&config, rt.clone())?;
        light_client.bootstrap()?;
        let keybase = KeyRing::new_secp256k1(Default::default(), "axon", &config.id).unwrap();
        Ok(Self {
            rt,
            config,
            keybase,
            light_client,
            tx_monitor_cmd: None,
            contract: todo!(),
            client: todo!(),
            conn_tx_hash: HashMap::new(),
        })
    }

    fn shutdown(self) -> Result<(), Error> {
        tracing::debug!("runtime of axon chain endpoint shutdown");
        Ok(())
    }

    fn health_check(&self) -> Result<HealthCheck, Error> {
        Ok(HealthCheck::Healthy)
    }

    fn subscribe(&mut self) -> Result<Subscription, Error> {
        let tx_monitor_cmd = match &self.tx_monitor_cmd {
            Some(tx_monitor_cmd) => tx_monitor_cmd,
            None => {
                let tx_monitor_cmd = self.init_event_monitor()?;
                self.tx_monitor_cmd = Some(tx_monitor_cmd);
                self.tx_monitor_cmd.as_ref().unwrap()
            }
        };

        let subscription = tx_monitor_cmd.subscribe().map_err(Error::event_monitor)?;
        Ok(subscription)
    }

    fn keybase(&self) -> &KeyRing<Self::SigningKeyPair> {
        &self.keybase
    }

    fn keybase_mut(&mut self) -> &mut KeyRing<Self::SigningKeyPair> {
        &mut self.keybase
    }

    fn get_signer(&self) -> Result<Signer, Error> {
        todo!()
    }

    fn ibc_version(&self) -> Result<Option<semver::Version>, Error> {
        todo!()
    }

    fn send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        todo!()
    }

    fn send_messages_and_wait_check_tx(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<Response>, Error> {
        todo!()
    }

    fn verify_header(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: &AnyClientState,
    ) -> Result<Self::LightBlock, Error> {
        todo!()
    }

    fn check_misbehaviour(
        &mut self,
        update: &UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        todo!()
    }

    fn query_balance(&self, key_name: Option<&str>, denom: Option<&str>) -> Result<Balance, Error> {
        todo!("cannot implement");
    }

    fn query_all_balances(&self, key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        todo!("cannot implement");
    }

    fn query_denom_trace(&self, hash: String) -> Result<DenomTrace, Error> {
        todo!("cannot implement");
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        todo!("How to implement");
    }

    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        todo!("How to implement");
    }

    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        let client_states: Vec<_> = self
            .rt
            .block_on(self.contract.get_client_states().call())
            .map_err(convert_err)?;
        let client_states = client_states
            .iter()
            .map(to_identified_any_client_state)
            .collect::<Result<Vec<IdentifiedAnyClientState>, Error>>()?;
        Ok(client_states)
    }

    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "not support client state query in specific height".to_string(),
            ));
        }
        let (client_state, _) = self
            .rt
            .block_on(
                self.contract
                    .get_client_state(request.client_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;

        let client_state = to_any_client_state(&client_state)?;
        Ok((client_state, None))
    }

    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        let client_id: String = request.client_id.to_string();
        let height = request.consensus_height;
        let height = HeightData {
            revision_number: height.revision_number(),
            revision_height: height.revision_height(),
        };
        let (consensus_state, _) = self
            .rt
            .block_on(self.contract.get_consensus_state(client_id, height).call())
            .map_err(convert_err)?;
        let consensus_state = to_any_consensus_state(&consensus_state)?;
        Ok((consensus_state, None))
    }

    fn query_consensus_state_heights(
        &self,
        request: QueryConsensusStateHeightsRequest,
    ) -> Result<Vec<Height>, Error> {
        let client_id = request.client_id;
        let heights: Vec<_> = self
            .rt
            .block_on(
                self.contract
                    .get_consensus_heights(client_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;
        let heights = heights
            .iter()
            .map(|height| Height::new(height.revision_number, height.revision_height))
            .collect::<Result<Vec<Height>, _>>()
            .map_err(|_| Error::invalid_height_no_source())?;
        Ok(heights)
    }

    fn query_upgraded_client_state(
        &self,
        request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        unimplemented!("not support")
    }

    fn query_upgraded_consensus_state(
        &self,
        request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        unimplemented!("not support")
    }

    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        let connections: Vec<_> = self
            .rt
            .block_on(self.contract.get_connections().call())
            .map_err(convert_err)?;
        let connections = connections
            .into_iter()
            .map(IdentifiedConnectionEnd::from)
            .collect();
        Ok(connections)
    }

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        let connection_ids: Vec<_> = self
            .rt
            .block_on(
                self.contract
                    .get_client_connections(request.client_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;
        let connection_ids = connection_ids
            .iter()
            .map(|id| ConnectionId::from_str(id.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Error::other_error(e.to_string()))?;
        Ok(connection_ids)
    }

    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "not support connection query in specific height".to_string(),
            ));
        }
        let (connection_end, _) = self
            .rt
            .block_on(
                self.contract
                    .get_connection(request.connection_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;
        let connection_end = connection_end.into();
        Ok((connection_end, None))
    }

    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        let channels: Vec<_> = self
            .rt
            .block_on(
                self.contract
                    .get_connection_channels(request.connection_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;
        let channels = channels
            .into_iter()
            .map(IdentifiedChannelEnd::from)
            .collect();
        Ok(channels)
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        let channels: Vec<_> = self
            .rt
            .block_on(self.contract.get_channels().call())
            .map_err(convert_err)?;
        let channels = channels
            .into_iter()
            .map(IdentifiedChannelEnd::from)
            .collect();
        Ok(channels)
    }

    fn query_channel(
        &self,
        request: QueryChannelRequest,
        include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "not support channel query in specific height".to_string(),
            ));
        }
        let (channel_end, _) = self
            .rt
            .block_on(
                self.contract
                    .get_channel(request.port_id.to_string(), request.channel_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;
        let channel_end = channel_end.into();
        Ok((channel_end, None))
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        let (client_state, found) = self
            .rt
            .block_on(
                self.contract
                    .get_channel_client_state(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;

        if !found {
            Ok(None)
        } else {
            let client_state = to_identified_any_client_state(&client_state)?;
            Ok(Some(client_state))
        }
    }

    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let (commitment, _) = self
            .rt
            .block_on(
                self.contract
                    .get_hashed_packet_commitment(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                        request.sequence.into(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;
        Ok((commitment.to_vec(), None))
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        let commitment_sequences = self
            .rt
            .block_on(
                self.contract
                    .get_hashed_packet_commitment_sequences(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;

        let commitment_sequences = commitment_sequences
            .iter()
            .map(|seq| (*seq).into())
            .collect();
        let height: Height = todo!();
        Ok((commitment_sequences, height))
    }

    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let has_receipt = self
            .rt
            .block_on(
                self.contract
                    .has_packet_receipt(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                        request.sequence.into(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;
        Ok((vec![has_receipt as u8], None))
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let mut sequences: Vec<Sequence> = vec![];
        for seq in request.packet_commitment_sequences {
            let has_receipt = self
                .rt
                .block_on(
                    self.contract
                        .has_packet_receipt(
                            request.port_id.to_string(),
                            request.channel_id.to_string(),
                            seq.into(),
                        )
                        .call(),
                )
                .map_err(convert_err)?;
            if !has_receipt {
                sequences.push(seq);
            }
        }
        Ok(sequences)
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "not support packet commitment query in specific height".to_string(),
            ));
        }
        let (commitment, _) = self
            .rt
            .block_on(
                self.contract
                    .get_hashed_packet_acknowledgement_commitment(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                        request.sequence.into(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;
        Ok((commitment.to_vec(), None))
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        let mut sequences: Vec<Sequence> = vec![];
        for seq in request.packet_commitment_sequences {
            let (_, found) = self
                .rt
                .block_on(
                    self.contract
                        .get_hashed_packet_acknowledgement_commitment(
                            request.port_id.to_string(),
                            request.channel_id.to_string(),
                            seq.into(),
                        )
                        .call(),
                )
                .map_err(convert_err)?;
            if found {
                sequences.push(seq);
            }
        }
        let height: Height = todo!("how to get height?");
        Ok((sequences, height))
    }

    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let mut sequences: Vec<Sequence> = vec![];
        for seq in request.packet_ack_sequences {
            let (_, found) = self
                .rt
                .block_on(
                    self.contract
                        .get_hashed_packet_acknowledgement_commitment(
                            request.port_id.to_string(),
                            request.channel_id.to_string(),
                            seq.into(),
                        )
                        .call(),
                )
                .map_err(convert_err)?;
            if !found {
                sequences.push(seq);
            }
        }
        Ok(sequences)
    }

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        let sequence = self
            .rt
            .block_on(
                self.contract
                    .get_next_sequence_recvs(
                        request.port_id.to_string(),
                        request.channel_id.to_string(),
                    )
                    .call(),
            )
            .map_err(convert_err)?;
        Ok((sequence.into(), None))
    }

    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        todo!()
    }

    fn query_packet_events(
        &self,
        request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        todo!()
    }

    fn query_host_consensus_state(
        &self,
        request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        todo!()
    }

    fn query_incentivized_packet(
        &self,
        request: ibc_proto::ibc::apps::fee::v1::QueryIncentivizedPacketRequest,
    ) -> Result<ibc_proto::ibc::apps::fee::v1::QueryIncentivizedPacketResponse, Error> {
        todo!()
    }

    fn build_client_state(
        &self,
        height: Height,
        settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        todo!()
    }

    fn build_consensus_state(
        &self,
        light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        todo!()
    }

    fn build_header(
        &mut self,
        trusted_height: Height,
        target_height: Height,
        client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        todo!()
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        channel_id: &ChannelId,
        port_id: &PortId,
        counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        todo!()
    }

    fn cross_chain_query(
        &self,
        requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        todo!()
    }

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error> {
        let message_type = match message_type {
            ConnectionMsgType::OpenTry => connection::State::Init,
            ConnectionMsgType::OpenAck => connection::State::TryOpen,
            ConnectionMsgType::OpenConfirm => connection::State::Open,
        };
        let key = (connection_id.clone(), message_type);
        let tx_hash = self.conn_tx_hash.get(&key).unwrap();
        let (tx, tx_receipt, block) = self.get_proof_ingredients(connection_id.clone(), *tx_hash)?;

        todo!("object_proof: build proof with transaction and block");
        todo!("client_proof: build client proof");
        todo!("assemble Proofs");
    }

    fn save_conn_tx_hash<T: Into<[u8; 32]>>(
        &mut self,
        connection_id: &ConnectionId,
        state: connection::State,
        tx_hash: T,
    ) -> Result<(), Error> {
        self.conn_tx_hash
            .insert((connection_id.clone(), state), tx_hash.into());
        Ok(())
    }
}

impl AxonChain {
    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        crate::time!("axon_init_event_monitor");
        let (create_receiver, header_receiver) = self.light_client.subscribe();
        let (event_monitor, monitor_tx) = AxonEventMonitor::new(
            self.config.id.clone(),
            self.config.websocket_addr.clone(),
            self.config.contract_address,
            header_receiver,
            self.rt.clone(),
        )
        .map_err(Error::event_monitor)?;
        thread::spawn(move || event_monitor.run());
        Ok(monitor_tx)
    }

    fn get_proof_ingredients<T: Send + Sync + Into<TxHash>>(
        &self,
        connection_id: ConnectionId,
        tx_hash: T,
    ) -> Result<(Transaction, TransactionReceipt, Block<TxHash>), Error> {
        let tx_hash = tx_hash.into();
        let tx = self
            .rt
            .block_on(self.client.get_transaction(tx_hash))
            .map_err(|e| Error::rpc_response(e.to_string()))?
            .ok_or_else(|| {
                Error::conn_open(
                    connection_id.clone(),
                    format!("can't find transaction with hash {}", hex::encode(tx_hash)),
                )
            })?;

        let tx_receipt = self
            .rt
            .block_on(self.client.get_transaction_receipt(tx_hash))
            .map_err(|e| Error::rpc_response(e.to_string()))?
            .ok_or_else(|| {
                Error::conn_open(
                    connection_id.clone(),
                    format!(
                        "can't find transaction receipt with hash {}",
                        hex::encode(tx_hash)
                    ),
                )
            })?;

        let block_number = tx.block_number.ok_or_else(|| {
            Error::conn_open(
                connection_id.clone(),
                format!("transaction {} is still pending", hex::encode(tx_hash)),
            )
        })?;
        let block =
            self.rt
                .block_on(self.client.get_block(BlockId::Number(
                    ethers::types::BlockNumber::Number(block_number),
                )))
                .map_err(|e| Error::rpc_response(e.to_string()))?
                .ok_or_else(|| {
                    Error::conn_open(
                        connection_id.clone(),
                        format!(
                            "can't find transaction receipt with hash {}",
                            hex::encode(tx_hash)
                        ),
                    )
                })?;
        Ok((tx, tx_receipt, block))
    }
}

fn convert_err(err: ethers_contract::ContractError<ContractProvider>) -> Error {
    Error::other_error(err.to_string())
}

fn to_identified_any_client_state(
    client_state: &ethers::core::types::Bytes,
) -> Result<IdentifiedAnyClientState, Error> {
    todo!("Type conversion. How to get specific consensus state from bytes?")
}

fn to_any_client_state(client_state: &ethers::core::types::Bytes) -> Result<AnyClientState, Error> {
    todo!("Type conversion. How to get specific consensus state from bytes?");
}

fn to_any_consensus_state(
    consensus_state: &ethers::core::types::Bytes,
) -> Result<AnyConsensusState, Error> {
    todo!("Type conversion.");
}

impl From<contract::ChannelCounterpartyData> for channel::Counterparty {
    fn from(value: contract::ChannelCounterpartyData) -> Self {
        Self {
            port_id: PortId::from_str(value.port_id.as_ref()).unwrap(),
            channel_id: if value.channel_id.is_empty() {
                None
            } else {
                Some(ChannelId::from_str(value.channel_id.as_ref()).unwrap())
            },
        }
    }
}

impl From<contract::ChannelData> for ChannelEnd {
    fn from(value: contract::ChannelData) -> Self {
        Self {
            state: channel::State::from_i32(value.state as i32).unwrap(),
            ordering: channel::Order::from_i32(value.ordering as i32).unwrap(),
            remote: value.counterparty.into(),
            connection_hops: value
                .connection_hops
                .iter()
                .map(|s| ConnectionId::from_str(s.as_ref()))
                .collect::<Result<Vec<ConnectionId>, _>>()
                .unwrap(),
            version: ics04_channel::version::Version::new(value.version),
        }
    }
}

impl From<contract::IdentifiedChannelData> for IdentifiedChannelEnd {
    fn from(value: contract::IdentifiedChannelData) -> Self {
        let channel_end = ChannelEnd {
            state: channel::State::from_i32(value.state as i32).unwrap(),
            ordering: channel::Order::from_i32(value.ordering as i32).unwrap(),
            remote: value.counterparty.into(),
            connection_hops: value
                .connection_hops
                .iter()
                .map(|s| ConnectionId::from_str(s.as_ref()))
                .collect::<Result<Vec<ConnectionId>, _>>()
                .unwrap(),
            version: ics04_channel::version::Version::new(value.version),
        };
        Self {
            port_id: PortId::from_str(value.port_id.as_ref()).unwrap(),
            channel_id: ChannelId::from_str(value.channel_id.as_ref()).unwrap(),
            channel_end,
        }
    }
}

impl From<contract::CounterpartyData> for connection::Counterparty {
    fn from(value: contract::CounterpartyData) -> Self {
        Self::new(
            ClientId::from_str(value.client_id.as_ref()).unwrap(),
            if value.connection_id.is_empty() {
                None
            } else {
                Some(ConnectionId::from_str(value.connection_id.as_ref()).unwrap())
            },
            CommitmentPrefix::try_from(value.prefix.key_prefix.as_ref().to_vec()).unwrap(),
        )
    }
}

impl From<contract::ConnectionEndData> for ConnectionEnd {
    fn from(value: contract::ConnectionEndData) -> Self {
        Self::new(
            connection::State::from_i32(value.state as i32).unwrap(),
            ClientId::from_str(value.client_id.as_ref()).unwrap(),
            value.counterparty.into(),
            value
                .versions
                .into_iter()
                .map(|v| ics03_connection::version::Version {
                    identifier: v.identifier,
                    features: v.features,
                })
                .collect::<Vec<_>>(),
            std::time::Duration::new(value.delay_period, 0),
        )
    }
}

impl From<contract::IdentifiedConnectionEndData> for IdentifiedConnectionEnd {
    fn from(value: contract::IdentifiedConnectionEndData) -> Self {
        Self {
            connection_id: ConnectionId::from_str(value.connection_id.as_ref()).unwrap(),
            connection_end: value.connection_end.into(),
        }
    }
}
