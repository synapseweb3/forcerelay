use std::{collections::HashMap, str::FromStr, sync::Arc, thread};

use axon_tools::types::{AxonBlock, Proof as AxonProof, Validator};
use eth2_types::Hash256;
use rlp::Encodable;
use tracing::warn;

use crate::{
    account::Balance,
    chain::{axon::contract::HeightData, requests::QueryHeight},
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, ChainConfig},
    connection::ConnectionMsgType,
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::{axon::LightClient as AxonLightClient, LightClient},
    misbehaviour::MisbehaviourEvidence,
};
use eth_light_client_in_ckb_prover::Receipts;
use ethers::{
    prelude::{k256::ecdsa::SigningKey, EthLogDecode, SignerMiddleware},
    providers::{Middleware, Provider, Ws},
    signers::{Signer as _, Wallet},
    types::{BlockNumber, TransactionRequest, TxHash, U64},
    utils::rlp,
};
use ibc_proto::{
    google::protobuf::Any,
    ibc::apps::fee::v1::{QueryIncentivizedPacketRequest, QueryIncentivizedPacketResponse},
};
use ibc_relayer_types::{
    applications::ics31_icq::response::CrossChainQueryResponse,
    clients::ics07_axon::{
        client_state::AxonClientState,
        consensus_state::AxonConsensusState,
        header::{AxonHeader, AXON_HEADER_TYPE_URL},
        light_block::AxonLightBlock,
    },
    core::{
        ics02_client::{
            error::Error as ClientError,
            events::UpdateClient,
            msgs::{create_client, update_client},
        },
        ics03_connection::{
            connection::{self, ConnectionEnd, IdentifiedConnectionEnd},
            msgs::{conn_open_ack, conn_open_confirm, conn_open_init, conn_open_try},
        },
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd, Order},
            msgs::{
                acknowledgement, chan_close_confirm, chan_close_init, chan_open_ack,
                chan_open_confirm, chan_open_init, chan_open_try, recv_packet, timeout,
            },
            packet::{PacketMsgType, Sequence},
        },
        ics23_commitment::{commitment::CommitmentPrefix, merkle::MerkleProof},
        ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId},
    },
    events::IbcEvent,
    proofs::{ConsensusProof, Proofs},
    signer::Signer,
    timestamp::Timestamp,
    tx_msg::Msg,
    Height,
};
use tendermint_rpc::endpoint::broadcast::tx_sync::Response;

use self::{contract::OwnableIBCHandler, monitor::AxonEventMonitor};

type ContractProvider = SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>;
type Contract = OwnableIBCHandler<ContractProvider>;

use super::{
    client::ClientSettings,
    cosmos::encode::key_pair_to_signer,
    endpoint::{ChainEndpoint, ChainStatus, HealthCheck},
    handle::{CacheTxHashStatus, Subscription},
    requests::{
        CrossChainQueryRequest, IncludeProof, QueryChannelClientStateRequest, QueryChannelRequest,
        QueryChannelsRequest, QueryClientConnectionsRequest, QueryClientStateRequest,
        QueryClientStatesRequest, QueryConnectionChannelsRequest, QueryConnectionRequest,
        QueryConnectionsRequest, QueryConsensusStateHeightsRequest, QueryConsensusStateRequest,
        QueryHostConsensusStateRequest, QueryNextSequenceReceiveRequest,
        QueryPacketAcknowledgementRequest, QueryPacketAcknowledgementsRequest,
        QueryPacketCommitmentRequest, QueryPacketCommitmentsRequest, QueryPacketEventDataRequest,
        QueryPacketReceiptRequest, QueryTxRequest, QueryUnreceivedAcksRequest,
        QueryUnreceivedPacketsRequest, QueryUpgradedClientStateRequest,
        QueryUpgradedConsensusStateRequest,
    },
    tracking::TrackedMsgs,
};
use tokio::runtime::Runtime as TokioRuntime;

mod contract;
mod monitor;
mod msg;
mod rpc;
mod utils;

pub use rpc::AxonRpc;
use utils::*;

use lazy_static::lazy_static;
use std::sync::Mutex;

// use lazy_static to replace in-object variables to speed up message transfer while
// counting on the process of cross-thread message is too slow to trigger some issues
lazy_static! {
    pub static ref CONNECTION_TX_HASH: Mutex<HashMap<(ChainId, ConnectionId), TxHash>> =
        Mutex::new(HashMap::new());
    pub static ref CHANNEL_TX_HASH: Mutex<HashMap<(ChainId, ChannelId, PortId), TxHash>> =
        Mutex::new(HashMap::new());
    pub static ref PACKET_TX_HASH: Mutex<HashMap<(ChainId, ChannelId, PortId, u64), TxHash>> =
        Mutex::new(HashMap::new());
}

pub struct AxonChain {
    rt: Arc<TokioRuntime>,
    config: AxonChainConfig,
    light_client: AxonLightClient,
    tx_monitor_cmd: Option<TxMonitorCmd>,
    contract: Contract,
    rpc_client: rpc::AxonRpcClient,
    client: Arc<ContractProvider>,
    keybase: KeyRing<Secp256k1KeyPair>,
}

impl ChainEndpoint for AxonChain {
    type LightBlock = AxonLightBlock;
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
        let axon_chain_id = rt
            .block_on(client.get_chainid())
            .map_err(|e| Error::other_error(e.to_string()))?;
        let key_entry = keybase.get_key(&config.key_name).map_err(Error::key_base)?;
        let wallet = key_entry
            .into_ether_wallet()
            .with_chain_id(axon_chain_id.as_u64());
        let client = Arc::new(SignerMiddleware::new(client, wallet));
        let contract = Contract::new(config.contract_address, Arc::clone(&client));
        let light_client = AxonLightClient::from_config(&config, rt.clone())?;

        // TODO: since Ckb endpoint uses Axon metadata cell as its light client, Axon
        //       endpoint has no need to monitor the update of its metadata
        //
        // let metadata = rt.block_on(rpc_client.get_current_metadata())?;
        // let epoch_len = metadata.version.end - metadata.version.start + 1;
        // light_client.bootstrap(client.clone(), rpc_client.clone(), epoch_len)?;

        Ok(Self {
            rt,
            config,
            keybase,
            light_client,
            tx_monitor_cmd: None,
            contract,
            rpc_client,
            client,
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
        let responses = self
            .send_messages_and_wait_commit(tracked_msgs)?
            .into_iter()
            .map(|event| {
                let value = event.to_string();
                let data = value.as_bytes().to_vec();
                Response {
                    code: tendermint::abci::Code::Ok,
                    data: data.into(),
                    log: String::new(),
                    hash: tendermint::Hash::Sha256(event.tx_hash),
                }
            })
            .collect();
        Ok(responses)
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

    fn query_balance(
        &self,
        _key_name: Option<&str>,
        _denom: Option<&str>,
    ) -> Result<Balance, Error> {
        // TODO: implement the real `query_balance` function later
        warn!("axon query_balance() cannot implement");
        Ok(Balance {
            amount: "100".to_owned(),
            denom: "AT".to_owned(),
        })
    }

    fn query_all_balances(&self, _key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        // TODO: implement the real `query_all_balances` function later
        warn!("axon query_all_balances() cannot implement");
        Ok(vec![])
    }

    fn query_denom_trace(&self, _hash: String) -> Result<DenomTrace, Error> {
        // TODO: implement the real `query_denom_trace` function later
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
        let tip_block = self
            .rt
            .block_on(self.client.get_block(BlockNumber::Latest))
            .map_err(|e| Error::rpc_response(e.to_string()))?;
        if let Some(block) = tip_block {
            let height = if let Some(number) = block.number {
                Height::from_noncosmos_height(number.as_u64())
            } else {
                Height::default()
            };
            Ok(ChainStatus {
                height,
                timestamp: to_timestamp(block.timestamp.as_u64())?,
            })
        } else {
            Ok(ChainStatus {
                height: Height::default(),
                timestamp: Timestamp::now(),
            })
        }
    }

    fn query_clients(
        &self,
        _request: QueryClientStatesRequest,
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
        _include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            warn!("query client state at specific height will fallback to latest");
        }
        let (client_state, _) = self
            .rt
            .block_on(
                self.contract
                    .get_client_state(request.client_id.to_string())
                    .call(),
            )
            .map_err(convert_err)?;

        let (_, client_state) = to_any_client_state(&client_state)?;
        Ok((client_state, None))
    }

    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        if matches!(request.query_height, QueryHeight::Specific(_)) {
            warn!("query consensus state at specific height will fallback to latest");
        }
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
        Ok((to_any_consensus_state(&consensus_state)?, None))
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
        _request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        unimplemented!("not support")
    }

    fn query_upgraded_consensus_state(
        &self,
        _request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        unimplemented!("not support")
    }

    fn query_connections(
        &self,
        _request: QueryConnectionsRequest,
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
        _include_proof: IncludeProof,
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
        _request: QueryChannelsRequest,
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
        _include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        if matches!(request.height, QueryHeight::Specific(_)) {
            // TODO: no implemention for specific channel query
            warn!("query channel at specific height will fallback to latest");
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

        if found {
            Ok(Some(to_identified_any_client_state(&client_state)?))
        } else {
            Ok(None)
        }
    }

    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        _include_proof: IncludeProof,
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
        Ok((commitment_sequences, Height::default()))
    }

    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        _include_proof: IncludeProof,
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
        if has_receipt {
            Ok((vec![1u8], None))
        } else {
            Ok((vec![], None))
        }
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let (channel, _) = self.query_channel(
            QueryChannelRequest {
                port_id: request.port_id.clone(),
                channel_id: request.channel_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )?;
        let mut sequences: Vec<Sequence> = vec![];
        if channel.ordering == Order::Ordered {
            let (max_recv_seq, _) = self.query_next_sequence_receive(
                QueryNextSequenceReceiveRequest {
                    port_id: request.port_id,
                    channel_id: request.channel_id,
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )?;
            sequences = request
                .packet_commitment_sequences
                .into_iter()
                .filter(|seq| *seq >= max_recv_seq)
                .collect();
        } else if channel.ordering == Order::Unordered {
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
        }
        Ok(sequences)
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
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
        Ok((sequences, Height::default()))
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
        _include_proof: IncludeProof,
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

    fn query_txs(&self, _request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        // TODO
        warn!("axon query_txs() not support");
        Ok(vec![])
    }

    fn query_packet_events(
        &self,
        _request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        // TODO
        warn!("axon query_packet_events() not support");
        Ok(vec![])
    }

    fn query_host_consensus_state(
        &self,
        _request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        // TODO
        warn!("axon query_host_consensus_state() not support");
        Ok(AxonConsensusState {})
    }

    fn query_incentivized_packet(
        &self,
        _request: QueryIncentivizedPacketRequest,
    ) -> Result<QueryIncentivizedPacketResponse, Error> {
        // TODO
        warn!("axon query_incentivized_packet() not support");
        Ok(QueryIncentivizedPacketResponse {
            incentivized_packet: None,
        })
    }

    fn build_client_state(
        &self,
        height: Height,
        settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        match settings {
            ClientSettings::AxonCkb | ClientSettings::Other => Ok(AxonClientState {
                chain_id: self.id(),
                latest_height: height,
            }),
            _ => Err(Error::build_client_state_failure()),
        }
    }

    fn build_consensus_state(
        &self,
        _light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        Ok(AxonConsensusState {})
    }

    fn build_header(
        &mut self,
        _trusted_height: Height,
        _target_height: Height,
        _client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        Ok((AxonHeader {}, vec![]))
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        _channel_id: &ChannelId,
        _port_id: &PortId,
        _counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        // TODO
        warn!("axon maybe_register_counterparty_payee() not support");
        Ok(())
    }

    fn cross_chain_query(
        &self,
        _requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        // TODO
        warn!("axon cross_chain_query() not support");
        Ok(vec![])
    }

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        _client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error> {
        let state = match message_type {
            ConnectionMsgType::OpenTry => connection::State::Init,
            ConnectionMsgType::OpenAck => connection::State::TryOpen,
            ConnectionMsgType::OpenConfirm => connection::State::Open,
        };
        let conn_tx_hash = CONNECTION_TX_HASH.lock().unwrap();
        let tx_hash = conn_tx_hash
            .get(&(self.id(), connection_id.clone()))
            .ok_or(Error::conn_proof(
                connection_id.clone(),
                format!("missing connection tx_hash, state {state:?}"),
            ))?;
        let proofs = self.get_proofs(tx_hash, height).map_err(|e| {
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
        let chan_tx_hash = CHANNEL_TX_HASH.lock().unwrap();
        let tx_hash = chan_tx_hash
            .get(&(self.id(), channel_id.clone(), port_id.clone()))
            .ok_or(Error::chan_proof(
                port_id.clone(),
                channel_id.clone(),
                "missing channel tx_hash".to_owned(),
            ))?;
        let proofs = self.get_proofs(tx_hash, height).map_err(|e| {
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
        let conn_tx_hash = PACKET_TX_HASH.lock().unwrap();
        let tx_hash = conn_tx_hash
            .get(&(
                self.id(),
                channel_id.clone(),
                port_id.clone(),
                sequence.into(),
            ))
            .ok_or(Error::packet_proof(
                port_id.clone(),
                channel_id.clone(),
                sequence.into(),
                format!(
                    "missing packet tx_hash on {packet_type}({channel_id}/{port_id}/{sequence})"
                ),
            ))?;
        let proofs = self.get_proofs(tx_hash, height).map_err(|e| {
            Error::chan_proof(
                port_id.clone(),
                channel_id.clone(),
                format!(
                    "{}, {packet_type}({channel_id}/{port_id}/{sequence})",
                    e.detail()
                ),
            )
        })?;
        Ok(proofs)
    }
}

impl AxonChain {
    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        crate::time!("axon_init_event_monitor");
        // let header_receiver = self.light_client.subscribe();
        let (event_monitor, monitor_tx) = AxonEventMonitor::new(
            self.config.id.clone(),
            self.config.websocket_addr.clone(),
            self.config.contract_address,
            // header_receiver,
            self.rt.clone(),
        )
        .map_err(Error::event_monitor)?;

        // restore past events to initialize tx_hash caches
        let latest_block_count = self.config.restore_block_count;
        event_monitor
            .restore_event_tx_hashes(latest_block_count)
            .map_err(Error::event_monitor)?
            .into_iter()
            .for_each(|v| cache_ics_tx_hash_with_event(self.id(), v.event, v.tx_hash));

        thread::spawn(move || event_monitor.run());
        Ok(monitor_tx)
    }

    fn get_proofs(&self, tx_hash: &TxHash, height: Height) -> Result<Proofs, Error> {
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

        let (block, state_root, block_proof, mut validators) = self
            .rt
            .block_on(self.get_proofs_ingredients(block_number))?;

        // FIXME: keep it commentted until Axon team fixed this verify issue
        //
        // check the validation of receipts mpt proof
        // let key = rlp::encode(&receipt.transaction_index.as_u64());
        // axon_tools::verify_trie_proof(block.header.receipts_root, &key, receipt_proof.clone())
        //     .map_err(|e| Error::rpc_response(format!("unverified receipts mpt: {e:?}")))?;

        let object_proof =
            to_ckb_like_object_proof(&receipt, &receipt_proof, &block, &state_root, &block_proof)
                .rlp_bytes()
                .to_vec();

        let useless_client_proof = vec![0u8].try_into().unwrap();
        let useless_consensus_proof =
            ConsensusProof::new(vec![0u8].try_into().unwrap(), Height::default()).unwrap();
        let proofs = Proofs::new(
            object_proof.try_into().unwrap(),
            Some(useless_client_proof),
            Some(useless_consensus_proof),
            None,
            height,
        )
        .unwrap();

        // check the validation of Axon block
        axon_tools::verify_proof(block, state_root, &mut validators, block_proof)
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
        // FIXME: maybe we won't get proof because the next block isn't mined yet, so here needs double check
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
}

macro_rules! convert {
    ($self:ident, $msg:ident, $eventy:ty, $method:ident) => {{
        let msg: $eventy = $msg.try_into()?;
        $self
            .rt
            .block_on(async { Ok($self.contract.$method(msg.clone()).send().await?.await?) })
    }};
}

impl AxonChain {
    fn send_message(&mut self, message: Any) -> Result<IbcEventWithHeight, Error> {
        use contract::*;
        let msg = message.clone();
        let tx_receipt: eyre::Result<_> = match msg.type_url.as_str() {
            // client
            create_client::TYPE_URL => {
                convert!(self, msg, MsgCreateClient, create_client)
            }
            // TODO: this update_client uses Hermes internal message to handle the Axon-specific function,
            // so maybe there is possibility to create a new one to do so
            update_client::TYPE_URL => {
                let msg = update_client::MsgUpdateClient::from_any(msg)
                    .map_err(|e| Error::protobuf_decode(update_client::TYPE_URL.to_string(), e))?;
                let bytes = msg.header.value.as_slice();
                let type_url = msg.header.type_url;
                let to = match type_url.as_str() {
                    AXON_HEADER_TYPE_URL => self.config.ckb_light_client_contract_address,
                    "CELL_TYPE_URL" => self.config.image_cell_contract_address,
                    type_url => {
                        return Err(Error::other_error(format!("unknown type_url {type_url}")))
                    }
                };
                let tx = TransactionRequest::new().to(to).data(bytes.to_vec());
                self.rt
                    .block_on(async { Ok(self.client.send_transaction(tx, None).await?.await?) })
            }
            // connection
            conn_open_init::TYPE_URL => {
                convert!(self, msg, MsgConnectionOpenInit, connection_open_init)
            }
            conn_open_try::TYPE_URL => {
                convert!(self, msg, MsgConnectionOpenTry, connection_open_try)
            }
            conn_open_ack::TYPE_URL => {
                convert!(self, msg, MsgConnectionOpenAck, connection_open_ack)
            }
            conn_open_confirm::TYPE_URL => {
                convert!(self, msg, MsgConnectionOpenConfirm, connection_open_confirm)
            }
            // channel
            chan_open_init::TYPE_URL => {
                convert!(self, msg, MsgChannelOpenInit, channel_open_init)
            }
            chan_open_try::TYPE_URL => {
                convert!(self, msg, MsgChannelOpenTry, channel_open_try)
            }
            chan_open_ack::TYPE_URL => {
                convert!(self, msg, MsgChannelOpenAck, channel_open_ack)
            }
            chan_open_confirm::TYPE_URL => {
                convert!(self, msg, MsgChannelOpenConfirm, channel_open_confirm)
            }
            chan_close_init::TYPE_URL => {
                convert!(self, msg, MsgChannelCloseInit, channel_close_init)
            }
            chan_close_confirm::TYPE_URL => {
                convert!(self, msg, MsgChannelCloseConfirm, channel_close_confirm)
            }
            // packet
            recv_packet::TYPE_URL => {
                convert!(self, msg, MsgPacketRecv, recv_packet)
            }
            acknowledgement::TYPE_URL => {
                convert!(self, msg, MsgPacketAcknowledgement, acknowledge_packet)
            }
            timeout::TYPE_URL => {
                let msg = {
                    let msg = timeout::MsgTimeout::from_any(msg.clone())
                        .map_err(|e| Error::protobuf_decode(timeout::TYPE_URL.into(), e))?;
                    // FIXME: add recv_timeout methond into solidity contract to handle this message type
                    recv_packet::MsgRecvPacket {
                        packet: msg.packet,
                        proofs: msg.proofs,
                        signer: msg.signer,
                    }
                };
                self.rt.block_on(async {
                    Ok(self.contract.recv_packet(msg.into()).send().await?.await?)
                })
            }
            url => {
                return Err(Error::other_error(format!(
                    "non-support message type url: {url}"
                )))
            }
        };
        let tx_receipt = tx_receipt
            .map_err(convert_err)?
            .ok_or(Error::send_tx(String::from("fail to send tx")))?;
        let event: IbcEvent = {
            use contract::OwnableIBCHandlerEvents::*;
            let mut events = tx_receipt
                .logs
                .into_iter()
                .map(Into::into)
                .map(|log| OwnableIBCHandlerEvents::decode_log(&log));
            match message.type_url.as_str() {
                create_client::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(CreateClientFilter(_))))
                }
                update_client::TYPE_URL => {
                    let msg = update_client::MsgUpdateClient::from_any(message).map_err(|e| {
                        Error::send_tx(format!("fail to decode MsgUpdateClient {}", e))
                    })?;
                    Some(Ok(UpdateClientFilter(contract::UpdateClientFilter {
                        client_id: msg.client_id.to_string(),
                        client_message: "update client".parse().unwrap(), // FIXME
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
                recv_packet::TYPE_URL | timeout::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(ReceivePacketFilter(_))))
                }
                acknowledgement::TYPE_URL => {
                    events.find(|event| matches!(event, Ok(AcknowledgePacketFilter(_))))
                }

                url => {
                    return Err(Error::send_tx(format!(
                        "non-support message type url: {url}"
                    )))
                }
            }
        }
        .ok_or_else(|| {
            Error::send_tx("not find right event from Axon transaction receipt".to_owned())
        })?
        .unwrap()
        .into();
        let tx_hash = tx_receipt.transaction_hash.0;
        let height = {
            let block_height = tx_receipt.block_number.ok_or_else(|| {
                Error::send_tx(format!(
                    "transaction {} is still pending",
                    hex::encode(tx_hash)
                ))
            })?;
            Height::from_noncosmos_height(block_height.as_u64())
        };
        cache_ics_tx_hash_with_event(self.id(), event.clone(), tx_hash);
        Ok(IbcEventWithHeight {
            event,
            height,
            tx_hash,
        })
    }
}

fn cache_ics_tx_hash<T: Into<[u8; 32]>>(
    chain_id: ChainId,
    cached_status: CacheTxHashStatus,
    tx_hash: T,
) {
    let hash: [u8; 32] = tx_hash.into();
    match cached_status {
        CacheTxHashStatus::Connection(conn_id) => {
            CONNECTION_TX_HASH
                .lock()
                .unwrap()
                .insert((chain_id, conn_id), hash.into());
        }
        CacheTxHashStatus::Channel(chan_id, port_id) => {
            CHANNEL_TX_HASH
                .lock()
                .unwrap()
                .insert((chain_id, chan_id, port_id), hash.into());
        }
        CacheTxHashStatus::Packet(chan_id, port_id, sequence) => {
            PACKET_TX_HASH
                .lock()
                .unwrap()
                .insert((chain_id, chan_id, port_id, sequence), hash.into());
        }
    }
}

pub fn cache_ics_tx_hash_with_event<T: Into<[u8; 32]>>(
    chain_id: ChainId,
    event: IbcEvent,
    tx_hash: T,
) {
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
    if let Some(tx_hash_status) = tx_hash_status {
        cache_ics_tx_hash(chain_id, tx_hash_status, tx_hash);
    }
}
