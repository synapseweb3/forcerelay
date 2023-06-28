#![allow(dead_code, unused_variables, unused_imports)]
#![allow(clippy::diverging_sub_expression)]

use std::{
    collections::HashMap,
    str::FromStr,
    sync::{self, Arc},
    thread,
};

use axon_tools::types::{AxonBlock, Proof as AxonProof, Validator};
use bytes::Bytes;
use eth2_types::Hash256;
use tracing::warn;

use crate::{
    account::Balance,
    chain::{
        axon::contract::{HeightData, UpdateClientFilter},
        requests::QueryHeight,
    },
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, filter::port, ChainConfig},
    connection::ConnectionMsgType,
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::{axon::LightClient as AxonLightClient, LightClient},
    misbehaviour::MisbehaviourEvidence,
    util::collate::collate,
};
use eth_light_client_in_ckb_prover::Receipts;
use eth_light_client_in_ckb_verification::trie;
use ethers::{
    abi::{AbiDecode, AbiEncode},
    contract::ContractError,
    prelude::{k256::ecdsa::SigningKey, EthLogDecode, SignerMiddleware},
    providers::{Middleware, Provider, Ws},
    signers::Wallet,
    types::{
        Block, BlockId, BlockNumber, Transaction, TransactionReceipt, TransactionRequest, TxHash,
        H160, U64,
    },
    utils::{rlp, rlp::Encodable},
};
use futures::TryFutureExt;
use ibc_proto::{google::protobuf::Any, ibc::core::channel::v1::IdentifiedChannel};
use ibc_relayer_types::{
    applications::ics31_icq::response::CrossChainQueryResponse,
    clients::{
        ics07_axon::{
            client_state::ClientState as AxonClientState,
            consensus_state::ConsensusState as AxonConsensusState,
            header::{Header as AxonHeader, AXON_HEADER_TYPE_URL},
        },
        ics07_ckb::client_state,
    },
    core::{
        ics02_client::{
            error::Error as ClientError,
            events::{NewBlock, UpdateClient},
            msgs::update_client,
        },
        ics03_connection::{
            self,
            connection::{self, ConnectionEnd, IdentifiedConnectionEnd},
            msgs::{conn_open_ack, conn_open_confirm, conn_open_init, conn_open_try},
        },
        ics04_channel::{
            self,
            channel::{self, ChannelEnd, IdentifiedChannelEnd},
            events::OpenInit,
            msgs::{
                acknowledgement, chan_close_confirm, chan_close_init, chan_open_ack,
                chan_open_confirm, chan_open_init, chan_open_try, recv_packet,
            },
            packet::{PacketMsgType, Sequence},
        },
        ics23_commitment::{
            commitment::{CommitmentPrefix, CommitmentProofBytes},
            merkle::MerkleProof,
        },
        ics24_host::identifier::{self, ChainId, ChannelId, ClientId, ConnectionId, PortId},
    },
    events::IbcEvent,
    proofs::{ConsensusProof, Proofs},
    signer::Signer,
    timestamp::Timestamp,
    tx_msg::Msg,
    Height,
};
use itertools::Itertools;
use tendermint_rpc::{endpoint::broadcast::tx_sync::Response, query};

use self::{
    contract::{OwnableIBCHandler, OwnableIBCHandlerEvents},
    monitor::AxonEventMonitor,
};

type ContractProvider = SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>;
type Contract = OwnableIBCHandler<ContractProvider>;
type ContractEvents = OwnableIBCHandlerEvents;

use super::{
    client::ClientSettings,
    cosmos::encode::key_pair_to_signer,
    endpoint::{ChainEndpoint, ChainStatus, HealthCheck},
    handle::{CacheTxHashStatus, Subscription},
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
mod msg;
mod rpc;

pub use rpc::AxonRpc;

pub struct AxonChain {
    rt: Arc<TokioRuntime>,
    config: AxonChainConfig,
    light_client: AxonLightClient,
    tx_monitor_cmd: Option<TxMonitorCmd>,
    contract: Contract,
    rpc_client: rpc::AxonRpcClient,
    client: Arc<ContractProvider>,
    keybase: KeyRing<Secp256k1KeyPair>,
    conn_tx_hash: HashMap<ConnectionId, TxHash>,
    chan_tx_hash: HashMap<(ChannelId, PortId), TxHash>,
    packet_tx_hash: HashMap<(ChannelId, PortId, u64), TxHash>,
}

// Allow temporarily for development. Should remove when work is done.
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
        let keybase = KeyRing::new_secp256k1(Default::default(), "axon", &config.id)
            .map_err(Error::key_base)?;

        let url = config.websocket_addr.clone();
        let rpc_client = rpc::AxonRpcClient::new(&config.rpc_addr);
        let client = rt
            .block_on(Provider::<Ws>::connect(url.to_string()))
            .map_err(|_| Error::web_socket(url.into()))?;
        let key_entry = keybase.get_key(&config.key_name).map_err(Error::key_base)?;
        let wallet = key_entry.into_ether_wallet();
        let client = Arc::new(SignerMiddleware::new(client, wallet));

        let contract = Contract::new(config.contract_address, Arc::clone(&client));

        let light_client = AxonLightClient::from_config(&config, rt.clone())?;
        let metadata = rt.block_on(rpc_client.get_current_metadata())?;
        let epoch_len = metadata.version.end - metadata.version.start + 1;
        light_client.bootstrap(client.clone(), rpc_client.clone(), epoch_len)?;

        Ok(Self {
            rt,
            config,
            keybase,
            light_client,
            tx_monitor_cmd: None,
            contract,
            rpc_client,
            client,
            conn_tx_hash: HashMap::new(),
            chan_tx_hash: HashMap::new(),
            packet_tx_hash: HashMap::new(),
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
        let key_entry = self
            .keybase()
            .get_key(&self.config.key_name)
            .map_err(Error::key_base)?;
        let signer = key_pair_to_signer(&key_entry)?;
        Ok(signer)
    }

    fn ibc_version(&self) -> Result<Option<semver::Version>, Error> {
        Ok(None)
    }

    fn send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        // every skipped request will be set empty, e.g. udpate_client
        if tracked_msgs.msgs.is_empty() {
            return Ok(vec![]);
        }
        tracked_msgs
            .msgs
            .into_iter()
            .map(|msg| self.send_message(msg))
            .collect::<Result<Vec<_>, _>>()
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
        self.light_client
            .verify(trusted, target, client_state)
            .map(|v| v.target)
    }

    fn check_misbehaviour(
        &mut self,
        update: &UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        self.light_client.check_misbehaviour(update, client_state)
    }

    fn query_balance(&self, key_name: Option<&str>, denom: Option<&str>) -> Result<Balance, Error> {
        warn!("axon query_balance() cannot implement");
        Ok(Balance {
            amount: "".to_owned(),
            denom: "".to_owned(),
        })
    }

    fn query_all_balances(&self, key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        warn!("axon query_all_balances() cannot implement");
        Ok(vec![])
    }

    fn query_denom_trace(&self, hash: String) -> Result<DenomTrace, Error> {
        warn!("axon query_denom_trace() cannot implement");
        Ok(DenomTrace {
            path: "".to_owned(),
            base_denom: "".to_owned(),
        })
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        CommitmentPrefix::try_from(self.config.store_prefix.as_bytes().to_vec())
            .map_err(|_| Error::ics02(ClientError::empty_prefix()))
    }

    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        // we don't care about axon's light client, so we should skip status check on light client
        let max_height = Height::new(u64::MAX, u64::MAX).map_err(Error::ics02)?;
        Ok(ChainStatus {
            height: max_height,
            timestamp: Timestamp::now(),
        })
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
        let height = Height::new(u64::MAX, u64::MAX).unwrap();
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
        let height = Height::new(u64::MAX, u64::MAX).unwrap();
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
        warn!("axon query_txs() not support");
        Ok(vec![])
    }

    fn query_packet_events(
        &self,
        request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        warn!("axon query_packet_events() not support");
        Ok(vec![])
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
        // NOTE: Temporarily skip unimplementation
        let header = self
            .rt
            .block_on(self.rpc_client.get_block_by_id(1.into()))?
            .header;
        Ok((header.into(), vec![]))
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        channel_id: &ChannelId,
        port_id: &PortId,
        counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        warn!("axon maybe_register_counterparty_payee() not support");
        Ok(())
    }

    fn cross_chain_query(
        &self,
        requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        warn!("axon cross_chain_query() not support");
        Ok(vec![])
    }

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error> {
        let state = match message_type {
            ConnectionMsgType::OpenTry => connection::State::Init,
            ConnectionMsgType::OpenAck => connection::State::TryOpen,
            ConnectionMsgType::OpenConfirm => connection::State::Open,
        };
        let tx_hash = self
            .conn_tx_hash
            .get(connection_id)
            .ok_or(Error::conn_proof(
                connection_id.clone(),
                format!("missing connection tx_hash, state {state:?}"),
            ))?;
        let proofs = self.get_proofs(tx_hash).map_err(|e| {
            Error::conn_proof(
                connection_id.clone(),
                format!("{}, state {state:?}", e.detail()),
            )
        })?;
        Ok((None, proofs))
    }

    fn build_channel_proofs(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<Proofs, Error> {
        let tx_hash = self
            .chan_tx_hash
            .get(&(channel_id.clone(), port_id.clone()))
            .ok_or(Error::chan_proof(
                port_id.clone(),
                channel_id.clone(),
                "missing channel tx_hash".to_owned(),
            ))?;
        let proofs = self.get_proofs(tx_hash).map_err(|e| {
            Error::chan_proof(port_id.clone(), channel_id.clone(), e.detail().to_string())
        })?;
        Ok(proofs)
    }

    fn build_packet_proofs(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: Height,
    ) -> Result<Proofs, Error> {
        let tx_hash = self
            .packet_tx_hash
            .get(&(channel_id.clone(), port_id.clone(), sequence.into()))
            .ok_or(Error::packet_proof(
                port_id.clone(),
                channel_id.clone(),
                sequence.into(),
                format!("missing packet tx_hash, type {packet_type:?}"),
            ))?;
        let proofs = self.get_proofs(tx_hash).map_err(|e| {
            Error::chan_proof(
                port_id.clone(),
                channel_id.clone(),
                format!("{}, type {packet_type:?}", e.detail()),
            )
        })?;
        Ok(proofs)
    }

    fn cache_ics_tx_hash<T: Into<[u8; 32]>>(
        &mut self,
        cached_status: CacheTxHashStatus,
        tx_hash: T,
    ) -> Result<(), Error> {
        let hash: [u8; 32] = tx_hash.into();
        match cached_status {
            CacheTxHashStatus::Connection(conn_id) => {
                self.conn_tx_hash.insert(conn_id, hash.into());
            }
            CacheTxHashStatus::Channel(chan_id, port_id) => {
                self.chan_tx_hash.insert((chan_id, port_id), hash.into());
            }
            CacheTxHashStatus::Packet(chan_id, port_id, sequence) => {
                self.packet_tx_hash
                    .insert((chan_id, port_id, sequence), hash.into());
            }
        }
        Ok(())
    }
}

impl AxonChain {
    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        crate::time!("axon_init_event_monitor");
        let header_receiver = self.light_client.subscribe();
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

    fn get_proofs(&self, tx_hash: &TxHash) -> Result<Proofs, Error> {
        let receipt = self
            .rt
            .block_on(self.client.get_transaction_receipt(*tx_hash))
            .map_err(|e| Error::rpc_response(e.to_string()))?
            .ok_or_else(|| {
                Error::other_error(format!(
                    "can't find transaction receipt with hash {}",
                    hex::encode(tx_hash)
                ))
            })?;

        let block_number = receipt.block_number.ok_or_else(|| {
            Error::other_error(format!(
                "transaction {} is still pending",
                hex::encode(tx_hash)
            ))
        })?;

        let block = self
            .rt
            .block_on(self.client.get_block(block_number))
            .map_err(|e| Error::rpc_response(e.to_string()))?
            .ok_or_else(|| {
                Error::other_error(format!("can't find block with number {}", block_number))
            })?;

        let tx_receipts = block
            .transactions
            .into_iter()
            .map(|tx_hash| {
                let receipt = self
                    .rt
                    .block_on(self.client.get_transaction_receipt(tx_hash));
                match receipt {
                    Ok(Some(receipt)) => Ok(receipt),
                    Ok(None) => Err(Error::other_error(format!(
                        "can't find transaction receipt with hash {}",
                        hex::encode(tx_hash)
                    ))),
                    Err(e) => Err(Error::rpc_response(e.to_string())),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let receipts: Receipts = tx_receipts.into();
        let receipt_proof = receipts.generate_proof(receipt.transaction_index.as_usize());

        let (block, state_root, proof, mut validators) = self
            .rt
            .block_on(self.get_proofs_ingredients(block_number))?;

        // check the validation of receipts mpt proof
        let key = rlp::encode(&receipt.transaction_index.as_u64());
        let verify_mpt = trie::verify_proof(
            &receipt_proof,
            block.header.receipts_root.as_bytes(),
            &key,
            &receipt.rlp_bytes(),
        );
        if !verify_mpt {
            return Err(Error::rpc_response("unverified receipts mpt".to_owned()));
        }

        let object_proof = rlp::RlpStream::new()
            .append(&receipt)
            .append_list::<Vec<_>, Vec<_>>(&receipt_proof)
            .append(&block)
            .append(&state_root)
            .append(&proof)
            .as_raw()
            .to_owned();
        let height = Height::new(1, 1).unwrap();
        let consensus_proof =
            ConsensusProof::new(vec![1u8].try_into().unwrap(), Height::new(1, 1).unwrap()).unwrap();
        let client_proof = vec![1u8].try_into().unwrap();
        let proofs = Proofs::new(
            object_proof.try_into().unwrap(),
            Some(client_proof),
            Some(consensus_proof),
            None,
            height,
        )
        .unwrap();

        // check the validation of Axon block
        axon_tools::verify_proof(block, state_root, &mut validators, proof)
            .map_err(|_| Error::rpc_response("unverified axon block".to_owned()))?;

        Ok(proofs)
    }

    async fn get_proofs_ingredients(
        &self,
        block_number: U64,
    ) -> Result<(AxonBlock, Hash256, AxonProof, Vec<Validator>), Error> {
        let previous_number = block_number
            .checked_sub(1u64.into())
            .expect("bad block_number");
        let next_number = block_number
            .checked_add(1u64.into())
            .expect("bad block_number");

        let block = self.rpc_client.get_block_by_id(block_number.into()).await?;
        let state_root = self
            .rpc_client
            .get_block_by_id(previous_number.into())
            .await?
            .header
            .state_root;
        // maybe we won't get proof because the next block isn't mined yet, so here needs double check
        let proof = self.rpc_client.get_proof_by_id(next_number.into()).await?;
        let validators = self
            .rpc_client
            .get_current_metadata()
            .await?
            .verifier_list
            .into_iter()
            .map(|v| Validator {
                bls_pub_key: v.bls_pub_key,
                address: v.address,
                propose_weight: v.propose_weight,
                vote_weight: v.vote_weight,
            })
            .collect::<Vec<_>>();

        Ok((block, state_root, proof, validators))
    }

    fn cache_ics_tx_hash_with_event<T: Into<[u8; 32]>>(
        &mut self,
        event: IbcEvent,
        tx_hash: T,
    ) -> Result<(), Error> {
        let tx_hash_status = match event {
            IbcEvent::OpenInitConnection(event) => Some(CacheTxHashStatus::new_with_conn(
                event.0.connection_id.unwrap(),
            )),
            IbcEvent::OpenTryConnection(event) => Some(CacheTxHashStatus::new_with_conn(
                event.0.connection_id.unwrap(),
            )),
            IbcEvent::OpenAckConnection(event) => Some(CacheTxHashStatus::new_with_conn(
                event.0.connection_id.unwrap(),
            )),
            IbcEvent::OpenConfirmConnection(event) => Some(CacheTxHashStatus::new_with_conn(
                event.0.connection_id.unwrap(),
            )),
            IbcEvent::OpenInitChannel(event) => Some(CacheTxHashStatus::new_with_chan(
                event.channel_id.unwrap(),
                event.port_id,
            )),
            IbcEvent::OpenTryChannel(event) => Some(CacheTxHashStatus::new_with_chan(
                event.channel_id.unwrap(),
                event.port_id,
            )),
            IbcEvent::OpenAckChannel(event) => Some(CacheTxHashStatus::new_with_chan(
                event.channel_id.unwrap(),
                event.port_id,
            )),
            IbcEvent::OpenConfirmChannel(event) => Some(CacheTxHashStatus::new_with_chan(
                event.channel_id.unwrap(),
                event.port_id,
            )),
            IbcEvent::SendPacket(event) => Some(CacheTxHashStatus::new_with_packet(
                event.packet.source_channel,
                event.packet.source_port,
                event.packet.sequence.into(),
            )),
            IbcEvent::ReceivePacket(event) => Some(CacheTxHashStatus::new_with_packet(
                event.packet.destination_channel,
                event.packet.destination_port,
                event.packet.sequence.into(),
            )),
            _ => None,
        };
        match tx_hash_status {
            Some(tx_hash_status) => {
                self.cache_ics_tx_hash(tx_hash_status, tx_hash)?;
            }
            None => {}
        }
        Ok(())
    }
}

impl AxonChain {
    fn send_message(&mut self, message: Any) -> Result<IbcEventWithHeight, Error> {
        let type_url = message.type_url.clone();
        let tx_receipt = match type_url.as_str() {
            update_client::TYPE_URL => {
                let msg = update_client::MsgUpdateClient::from_any(message).map_err(|e| {
                    Error::other_error(format!("fail to decode MsgUpdateClient {}", e))
                })?;
                let bytes = msg.header.value.as_slice();
                let type_url = msg.header.type_url;
                let to = match type_url.as_str() {
                    AXON_HEADER_TYPE_URL => self.config.ckb_light_client_contract_address,
                    "CELL_TYPE_URL" => self.config.image_cell_contract_address,
                    type_url => {
                        return Err(Error::other_error(format!("unknown type_url {}", type_url)))
                    }
                };

                let tx = TransactionRequest::new().to(to).data(bytes.to_vec());
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> = self
                    .rt
                    .block_on(async { Ok(self.client.send_transaction(tx, None).await?.await?) });
                tx_receipt.map_err(convert_err)?
            }
            conn_open_init::TYPE_URL => {
                let msg: contract::MsgConnectionOpenInit = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .connection_open_init(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            conn_open_try::TYPE_URL => {
                let msg: contract::MsgConnectionOpenTry = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .connection_open_try(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            conn_open_ack::TYPE_URL => {
                let msg: contract::MsgConnectionOpenAck = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .connection_open_ack(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            conn_open_confirm::TYPE_URL => {
                let msg: contract::MsgConnectionOpenConfirm = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .connection_open_confirm(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_open_init::TYPE_URL => {
                let msg: contract::MsgChannelOpenInit = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_open_init(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_open_try::TYPE_URL => {
                let msg: contract::MsgChannelOpenTry = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_open_try(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_open_ack::TYPE_URL => {
                let msg: contract::MsgChannelOpenAck = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_open_ack(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_open_confirm::TYPE_URL => {
                let msg: contract::MsgChannelOpenConfirm = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_open_confirm(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_close_init::TYPE_URL => {
                let msg: contract::MsgChannelCloseInit = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_close_init(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            chan_close_confirm::TYPE_URL => {
                let msg: contract::MsgChannelCloseConfirm = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .channel_close_confirm(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            recv_packet::TYPE_URL => {
                let msg: contract::MsgPacketRecv = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self.contract.recv_packet(msg.clone()).send().await?.await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            acknowledgement::TYPE_URL => {
                let msg: contract::MsgPacketAcknowledgement = message.try_into()?;
                let tx_receipt: eyre::Result<Option<TransactionReceipt>> =
                    self.rt.block_on(async {
                        Ok(self
                            .contract
                            .acknowledge_packet(msg.clone())
                            .send()
                            .await?
                            .await?)
                    });
                tx_receipt.map_err(convert_err)?
            }
            url => {
                return Err(Error::other_error(format!(
                    "not support message type url: {}",
                    url
                )))
            }
        };
        let tx_receipt = tx_receipt.ok_or(Error::send_tx(String::from("fail to send tx")))?;
        let event: IbcEvent = {
            use contract::OwnableIBCHandlerEvents::*;
            let mut events = tx_receipt
                .logs
                .into_iter()
                .map(Into::into)
                .map(|log| OwnableIBCHandlerEvents::decode_log(&log));
            match type_url.as_str() {
                update_client::TYPE_URL => {
                    Some(Ok(UpdateClientFilter(contract::UpdateClientFilter {
                        client_id: "default-0".to_string(),
                        client_message: "0x12345678".parse().unwrap(),
                    })))
                }
                conn_open_init::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenInitConnectionFilter(_))))
                }
                conn_open_try::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenTryConnectionFilter(_))))
                }
                conn_open_ack::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenAckConnectionFilter(_))))
                }
                conn_open_confirm::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenConfirmConnectionFilter(_))))
                }
                chan_open_init::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenInitChannelFilter(_))))
                }
                chan_open_try::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenTryChannelFilter(_))))
                }
                chan_open_ack::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenAckChannelFilter(_))))
                }
                chan_open_confirm::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(OpenConfirmChannelFilter(_))))
                }
                chan_close_init::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(CloseInitChannelFilter(_))))
                }
                chan_close_confirm::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(CloseConfirmChannelFilter(_))))
                }
                recv_packet::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(ReceivePacketFilter(_))))
                }
                acknowledgement::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(AcknowledgePacketFilter(_))))
                }
                url => {
                    return Err(Error::other_error(format!(
                        "not support message type url: {}",
                        url
                    )))
                }
            }
        }
        .ok_or_else(|| {
            Error::other_error(String::from(
                "not find right event from AXON transaction receipt",
            ))
        })?
        .unwrap()
        .into();
        let tx_hash = tx_receipt.transaction_hash.0;
        let height = {
            let block_height = tx_receipt.block_number.ok_or_else(|| {
                Error::other_error(format!(
                    "transaction {} is still pending",
                    hex::encode(tx_hash)
                ))
            })?;
            Height::new(u64::MAX, block_height.as_u64()).unwrap()
        };
        let _ = self.cache_ics_tx_hash_with_event(event.clone(), tx_hash)?;
        Ok(IbcEventWithHeight {
            event,
            height,
            tx_hash,
        })
    }
}

fn convert_err<T: ToString>(err: T) -> Error {
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
