use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use axon_tools::types::{AxonBlock, Proof as AxonProof, ValidatorExtend};
use eth2_types::Hash256;
use k256::ecdsa::SigningKey;
use rlp::Encodable;
use tracing::{debug, warn};

use crate::{
    account::Balance,
    chain::{
        axon::contract::HeightData,
        requests::{Qualified, QueryHeight},
    },
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, ChainConfig},
    connection::ConnectionMsgType,
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    ibc_contract::OwnableIBCHandlerEvents,
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::{axon::LightClient as AxonLightClient, LightClient},
    misbehaviour::MisbehaviourEvidence,
};
use eth_light_client_in_ckb_prover::Receipts;
use ethers::{
    prelude::*,
    providers::{Middleware, Provider, Ws},
    signers::{Signer as _, Wallet},
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
        ics23_commitment::{
            commitment::{CommitmentPrefix, CommitmentRoot},
            merkle::MerkleProof,
        },
        ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId},
    },
    events::{IbcEvent, WithBlockDataType},
    proofs::{ConsensusProof, Proofs},
    signer::Signer,
    timestamp::Timestamp,
    tx_msg::Msg,
    Height,
};
use tendermint_rpc::endpoint::broadcast::tx_sync::Response;

use self::{contract::OwnableIBCHandler, monitor::AxonEventMonitor};

type ContractProvider = SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>;
type IBCContract = OwnableIBCHandler<ContractProvider>;
type ERC20Contract = ERC20<ContractProvider>;
type ICS20TransferERC20Contract = ICS20TransferERC20<ContractProvider>;

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
    SEC_TO_NANO,
};
use tokio::runtime::Runtime as TokioRuntime;

pub mod contract;
mod monitor;
mod msg;
mod rpc;
pub mod utils;

pub use rpc::AxonRpc;
use utils::*;

abigen!(
    ERC20,
    r"[
        function totalSupply() external view returns (uint256)
        function balanceOf(address account) external view returns (uint256)
        function transfer(address to, uint256 amount) external returns (bool)
        function allowance(address owner, address spender) external view returns (uint256)
        function approve(address spender, uint256 amount) external returns (bool)
        function transferFrom(address from, address to, uint256 amount) external returns (bool)
    ]"
);

// mapping(bytes32 => string) public denomTraces;
abigen!(
    ICS20TransferERC20,
    r"[
        function denomTraces(bytes32 hash) external view returns (string)
    ]"
);

#[derive(Default)]
pub struct IBCInfoCache {
    conn_tx_hash: HashMap<ConnectionId, TxHash>,
    chan_tx_hash: HashMap<(ChannelId, PortId), TxHash>,
    packet_tx_hash: HashMap<(ChannelId, PortId, u64), TxHash>,
}

pub struct AxonChain {
    rt: Arc<TokioRuntime>,
    config: AxonChainConfig,
    light_client: AxonLightClient,
    tx_monitor_cmd: Option<TxMonitorCmd>,
    rpc_client: rpc::AxonRpcClient,
    client: Provider<Ws>,
    keybase: KeyRing<Secp256k1KeyPair>,
    chain_id: u64,
    ibc_cache: Arc<RwLock<IBCInfoCache>>,
}

impl AxonChain {
    fn get_wallet(&self, key_name: &str) -> Result<Wallet<SigningKey>, Error> {
        let key_entry = self.keybase.get_key(key_name).map_err(Error::key_base)?;
        let wallet = key_entry.into_ether_wallet().with_chain_id(self.chain_id);
        Ok(wallet)
    }

    fn contract_provider(&self) -> Result<Arc<ContractProvider>, Error> {
        let wallet = self.get_wallet(&self.config.key_name)?;
        Ok(Arc::new(SignerMiddleware::new(self.client.clone(), wallet)))
    }

    fn contract(&self) -> Result<IBCContract, Error> {
        Ok(IBCContract::new(
            self.config.contract_address,
            self.contract_provider()?,
        ))
    }

    fn transfer_contract(&self) -> Result<ICS20TransferERC20Contract, Error> {
        Ok(ICS20TransferERC20::new(
            self.config.transfer_contract_address,
            self.contract_provider()?,
        ))
    }

    fn erc20_contract(&self, address: H160) -> Result<ERC20Contract, Error> {
        Ok(ERC20::new(address, self.contract_provider()?))
    }
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
        let chain_id = rt
            .block_on(client.get_chainid())
            .map_err(|e| Error::other_error(e.to_string()))?
            .as_u64();
        let light_client = AxonLightClient::from_config(&config, rt.clone())?;
        let ibc_cache = Arc::new(RwLock::new(IBCInfoCache::default()));

        // TODO: since Ckb endpoint uses Axon metadata cell as its light client, Axon
        //       endpoint has no need to monitor the update of its metadata
        //
        // let metadata = rt.block_on(rpc_client.get_current_metadata())?;
        // let epoch_len = metadata.version.end - metadata.version.start + 1;
        // light_client.bootstrap(client.clone(), rpc_client.clone(), epoch_len)?;

        // FIXME remove the light client or fully implement it

        Ok(Self {
            rt,
            config,
            keybase,
            light_client,
            tx_monitor_cmd: None,
            chain_id,
            rpc_client,
            client,
            ibc_cache,
        })
    }

    fn shutdown(self) -> Result<(), Error> {
        tracing::debug!("runtime of axon chain endpoint shutdown");
        if let Some(monitor_tx) = self.tx_monitor_cmd {
            monitor_tx.shutdown().map_err(Error::event_monitor)?;
        }
        Ok(())
    }

    fn health_check(&self) -> Result<HealthCheck, Error> {
        match self.rt.block_on(self.rpc_client.get_current_metadata()) {
            Ok(_) => Ok(HealthCheck::Healthy),
            Err(err) => Ok(HealthCheck::Unhealthy(Box::new(err))),
        }
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
        // TODO @jjy
        // The cosmos implementation simply returns the application version
        // We want the version to imply the supported ibc feature,
        // so IMO the best choice is using IBC solidity contract to store the version.
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

    // TODO the light client is unimplemented
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

    // TODO the light client is unimplemented
    fn check_misbehaviour(
        &mut self,
        update: &UpdateClient,
        client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        self.light_client.check_misbehaviour(update, client_state)
    }

    fn query_balance(&self, key_name: Option<&str>, denom: Option<&str>) -> Result<Balance, Error> {
        let key_name = key_name.unwrap_or(&self.config.key_name);
        let denom: &str =
            denom.ok_or_else(|| Error::other_error("do not support default denom".into()))?;
        let erc20_address = {
            let denom = denom.trim_start_matches("0x");
            let bytes = hex::decode(denom).map_err(Error::other)?;
            H160::from_slice(&bytes)
        };
        let contract = self.erc20_contract(erc20_address)?;
        let wallet = self.get_wallet(key_name)?;
        let amount = self
            .rt
            .block_on(contract.balance_of(wallet.address()).call())
            .map_err(|err| Error::query(format!("{err:?}")))?;

        Ok(Balance {
            amount: format!("{amount:#x}"),
            denom: denom.to_string(),
        })
    }

    // FIXME implement this after use a real ics token contract
    fn query_all_balances(&self, _key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        // TODO: implement the real `query_all_balances` function later
        warn!("axon query_all_balances() cannot implement");
        Ok(vec![])
    }

    fn query_denom_trace(&self, hash: String) -> Result<DenomTrace, Error> {
        let hash_bytes = H256::from_str(hash.trim_start_matches("ibc/")).map_err(Error::other)?;
        let contract = self.transfer_contract().map_err(Error::other)?;
        let full_path: String = self
            .rt
            .block_on(contract.denom_traces(hash_bytes.into()).call())
            .map_err(|err| Error::query(format!("{err:?}")))?;
        if full_path.is_empty() {
            return Err(Error::empty_denom_trace(hash));
        }
        let dt: DenomTrace = parse_denom_trace(full_path)?;
        Ok(dt)
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
            .block_on(self.contract()?.get_client_states().call())
            .map_err(convert_err)?;
        let client_states = client_states
            .iter()
            .map(to_identified_any_client_state)
            .collect::<Result<Vec<IdentifiedAnyClientState>, Error>>()?;
        Ok(client_states)
    }

    // TODO verify proof
    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        let mut call_builder = self
            .contract()?
            .get_client_state(request.client_id.to_string());
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height())
        }
        let (client_state, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;

        let (_, client_state) = to_any_client_state(&client_state)?;
        Ok((client_state, None))
    }

    // TODO verify proof
    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        let client_id: String = request.client_id.to_string();
        let height = {
            let height = request.consensus_height;
            HeightData {
                revision_number: height.revision_number(),
                revision_height: height.revision_height(),
            }
        };
        let mut call_builder = self.contract()?.get_consensus_state(client_id, height);
        if let QueryHeight::Specific(height) = request.query_height {
            call_builder = call_builder.block(height.revision_height());
        }
        let (consensus_state, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
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
                self.contract()?
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

    // TODO do we need to implement this?
    fn query_upgraded_client_state(
        &self,
        _request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        unimplemented!("not support")
    }

    // TODO do we need to implement this?
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
            .block_on(self.contract()?.get_connections().call())
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
                self.contract()?
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

    // TODO verify proof
    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        _include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        let mut call_builder = self
            .contract()?
            .get_connection(request.connection_id.to_string());
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height());
        }
        let (connection_end, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
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
                self.contract()?
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
            .block_on(self.contract()?.get_channels().call())
            .map_err(convert_err)?;
        let channels = channels
            .into_iter()
            .map(IdentifiedChannelEnd::from)
            .collect();
        Ok(channels)
    }

    // TODO verify proof
    fn query_channel(
        &self,
        request: QueryChannelRequest,
        _include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        let mut call_builder = self
            .contract()?
            .get_channel(request.port_id.to_string(), request.channel_id.to_string());
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height())
        }

        let (channel_end, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
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
                self.contract()?
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

    // TODO verify proof
    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let mut call_builder = self.contract()?.get_hashed_packet_commitment(
            request.port_id.to_string(),
            request.channel_id.to_string(),
            request.sequence.into(),
        );
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height());
        }
        let (commitment, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
        Ok((commitment.to_vec(), None))
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        let commitment_sequences = self
            .rt
            .block_on(
                self.contract()?
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

    // TODO verify proof
    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let mut call_builder = self.contract()?.has_packet_receipt(
            request.port_id.to_string(),
            request.channel_id.to_string(),
            request.sequence.into(),
        );
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height());
        }
        let has_receipt = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
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
                        self.contract()?
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

    // TODO verify proof
    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let mut call_builder = self
            .contract()?
            .get_hashed_packet_acknowledgement_commitment(
                request.port_id.to_string(),
                request.channel_id.to_string(),
                request.sequence.into(),
            );
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height());
        }
        let (commitment, _) = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
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
                    self.contract()?
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
            // The packet hasn't been acknowledged if packet commitment is
            // found. (Packet commitment is deleted after the packet is
            // acknowledged.)
            let (_, found) = self
                .rt
                .block_on(
                    self.contract()?
                        .get_hashed_packet_commitment(
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
        Ok(sequences)
    }

    // TODO verify proof
    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        let mut call_builder = self
            .contract()?
            .get_next_sequence_recvs(request.port_id.to_string(), request.channel_id.to_string());
        if let QueryHeight::Specific(height) = request.height {
            call_builder = call_builder.block(height.revision_height());
        }
        let sequence = self.rt.block_on(call_builder.call()).map_err(convert_err)?;
        Ok((sequence.into(), None))
    }

    // TODO do we need to implement this?
    fn query_txs(&self, _request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        warn!("axon query_txs() not support");
        Ok(vec![])
    }

    fn query_packet_events(
        &self,
        request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        let QueryPacketEventDataRequest {
            event_id,
            source_channel_id,
            source_port_id,
            destination_channel_id,
            destination_port_id,
            sequences,
            height,
        } = request;

        let mut filter = Filter::new().address(self.config.contract_address);
        // filter height
        match height {
            Qualified::SmallerEqual(QueryHeight::Latest)
            | Qualified::Equal(QueryHeight::Latest) => {
                // until the latest block
            }
            Qualified::SmallerEqual(QueryHeight::Specific(height)) => {
                filter = filter.to_block(height.revision_height());
            }
            Qualified::Equal(QueryHeight::Specific(height)) => {
                filter = filter
                    .from_block(height.revision_height())
                    .to_block(height.revision_height());
            }
        }

        let logs = self
            .rt
            .block_on(self.client.get_logs(&filter))
            .map_err(|e| Error::other_error(e.to_string()))?;

        let logs_iter = logs.into_iter().map(|log| {
            let height = {
                let number = log.block_number.expect("no block number").as_u64();
                Height::from_noncosmos_height(number)
            };
            let tx_hash: [u8; 32] = log.transaction_hash.expect("no tx hash").into();
            let event = OwnableIBCHandlerEvents::decode_log(&log.into()).expect("parse log");
            (height, tx_hash, event)
        });

        let packet_filter = |packet: &contract::PacketData| {
            if !sequences.is_empty() && !sequences.contains(&Sequence::from(packet.sequence)) {
                return false;
            }
            if packet.destination_channel != destination_channel_id.to_string() {
                return false;
            }
            if packet.source_channel != source_channel_id.to_string() {
                return false;
            }
            if packet.destination_port != destination_port_id.to_string() {
                return false;
            }
            if packet.source_port != source_port_id.to_string() {
                return false;
            }
            true
        };

        let events: Vec<_> = match event_id {
            WithBlockDataType::CreateClient => logs_iter
                .filter_map(|(height, tx_hash, event)| {
                    if matches!(event, OwnableIBCHandlerEvents::CreateClientFilter(..)) {
                        ibc_event_from_ibc_handler_event(height, tx_hash, event)
                            .ok()
                            .unwrap_or(None)
                    } else {
                        None
                    }
                })
                .collect(),
            WithBlockDataType::UpdateClient => logs_iter
                .filter_map(|(height, tx_hash, event)| {
                    if matches!(event, OwnableIBCHandlerEvents::UpdateClientFilter(..)) {
                        ibc_event_from_ibc_handler_event(height, tx_hash, event)
                            .ok()
                            .unwrap_or(None)
                    } else {
                        None
                    }
                })
                .collect(),
            WithBlockDataType::SendPacket => logs_iter
                .filter_map(|(height, tx_hash, event)| {
                    if let OwnableIBCHandlerEvents::SendPacketFilter(contract::SendPacketFilter {
                        packet,
                    }) = &event
                    {
                        if !packet_filter(packet) {
                            return None;
                        }
                        ibc_event_from_ibc_handler_event(height, tx_hash, event)
                            .ok()
                            .unwrap_or(None)
                    } else {
                        None
                    }
                })
                .collect(),
            WithBlockDataType::WriteAck => logs_iter
                .filter_map(|(height, tx_hash, event)| {
                    if let OwnableIBCHandlerEvents::WriteAcknowledgementFilter(
                        contract::WriteAcknowledgementFilter { packet, .. },
                    ) = &event
                    {
                        if !packet_filter(packet) {
                            return None;
                        }
                        ibc_event_from_ibc_handler_event(height, tx_hash, event)
                            .ok()
                            .unwrap_or(None)
                    } else {
                        None
                    }
                })
                .collect(),
        };

        tracing::debug!("Axon filtered {} packet events", events.len());
        Ok(events)
    }

    fn query_host_consensus_state(
        &self,
        request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        let fut = match request.height {
            QueryHeight::Latest => self
                .rpc_client
                .get_block_by_id(BlockId::Number(BlockNumber::Latest)),
            QueryHeight::Specific(ibc_height) => {
                let number = ibc_height.revision_height();
                self.rpc_client
                    .get_block_by_id(BlockId::Number(BlockNumber::Number(number.into())))
            }
        };
        let block = self
            .rt
            .block_on(fut)?
            .ok_or_else(|| Error::invalid_height_no_source())?;
        let root = CommitmentRoot::from_bytes(block.header.state_root.as_bytes());
        let timestamp = Timestamp::from_nanoseconds(block.header.timestamp * SEC_TO_NANO)
            .map_err(Error::other)?;
        Ok(AxonConsensusState { root, timestamp })
    }

    // TODO do we need to implement this?
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

    // TODO do we need to implement this?
    fn build_consensus_state(
        &self,
        _light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        Ok(AxonConsensusState {
            root: CommitmentRoot::from_bytes(&[]),
            timestamp: Timestamp::default(),
        })
    }

    // TODO do we need to implement this?
    fn build_header(
        &mut self,
        _trusted_height: Height,
        _target_height: Height,
        _client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        Ok((AxonHeader {}, vec![]))
    }

    // TODO do we need to implement this?
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

    // TODO do we need to implement this?
    fn cross_chain_query(
        &self,
        _requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        // TODO
        warn!("axon cross_chain_query() not support");
        Ok(vec![])
    }

    // TODO do we need to implement this?
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
        let ibc_cache = self.ibc_cache.read().unwrap();
        let tx_hash = ibc_cache
            .conn_tx_hash
            .get(connection_id)
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
        let ibc_cache = self.ibc_cache.read().unwrap();
        let tx_hash = ibc_cache
            .chan_tx_hash
            .get(&(channel_id.clone(), port_id.clone()))
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
        let ibc_cache = self.ibc_cache.read().unwrap();
        let tx_hash = ibc_cache
            .packet_tx_hash
            .get(&(channel_id.clone(), port_id.clone(), sequence.into()))
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

/// Modified from ibc-go https://github.com/cosmos/ibc-go/blob/main/modules/apps/transfer/types/trace.go#L31
fn parse_denom_trace(raw_denom: String) -> Result<DenomTrace, Error> {
    let parts: Vec<_> = raw_denom.split('/').collect();
    if parts[0] == raw_denom {
        return Ok(DenomTrace {
            path: Default::default(),
            base_denom: raw_denom,
        });
    }
    let (path, base_denom) = extract_path_and_base_from_full_denom(parts);
    Ok(DenomTrace { path, base_denom })
}

fn extract_path_and_base_from_full_denom(parts: Vec<&str>) -> (String, String) {
    fn is_valid_channel_id(c: &str) -> bool {
        const PREFIX: &str = "channel-";
        if !c.starts_with(PREFIX) {
            return false;
        }
        let r = c[PREFIX.len()..].parse::<usize>();
        r.is_ok()
    }

    let mut path = Vec::new();
    let mut base = Vec::new();
    let len = parts.len();

    for i in (0..len).step_by(2) {
        if i < len - 1 && len > 2 && is_valid_channel_id(parts[i + 1]) {
            path.push(parts[i]);
            path.push(parts[i + 1]);
        } else {
            base.extend_from_slice(&parts[i..]);
            break;
        }
    }
    let path = path.join("/");
    let base = base.join("/");

    (path, base)
}

impl AxonChain {
    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        crate::time!("axon_init_event_monitor");
        // let header_receiver = self.light_client.subscribe();
        let ibc_cache = self.ibc_cache.clone();
        let (mut event_monitor, monitor_tx) = AxonEventMonitor::new(
            self.config.id.clone(),
            self.config.websocket_addr.clone(),
            self.config.contract_address,
            // header_receiver,
            self.rt.clone(),
            ibc_cache.clone(),
        )
        .map_err(Error::event_monitor)?;

        // restore past events to initialize tx_hash caches
        let mut ibc_cache = ibc_cache.write().unwrap();
        let latest_block_count = self.config.restore_block_count;
        event_monitor
            .restore_event_tx_hashes(latest_block_count)
            .map_err(Error::event_monitor)?
            .into_iter()
            .for_each(|v| cache_ics_tx_hash_with_event(&mut ibc_cache, v.event, v.tx_hash));

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

        // check the validation of receipts mpt proof
        let key = rlp::encode(&receipt.transaction_index.as_u64());
        let result =
            axon_tools::verify_trie_proof(block.header.receipts_root, &key, receipt_proof.clone())
                .map_err(|e| Error::rpc_response(format!("unverified receipts mpt: {e:?}")))?;
        if result.is_none() {
            return Err(Error::rpc_response(format!(
                "trie key: {} doesn't exist",
                receipt.transaction_index
            )));
        }

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
        let debug_content = generate_debug_content(&block, &state_root, &block_proof, &validators);

        // check the validation of Axon block
        axon_tools::verify_proof(block, state_root, &mut validators, block_proof).map_err(
            |err| {
                std::fs::write(
                    format!("./debug/axon_block_{block_number}.log"),
                    debug_content,
                )
                .unwrap();
                let err_msg = format!("unverified axon block #{block_number}, err: {:?}", err);
                Error::rpc_response(err_msg)
            },
        )?;

        Ok(proofs)
    }

    async fn get_proofs_ingredients(
        &self,
        block_number: U64,
    ) -> Result<(AxonBlock, Hash256, AxonProof, Vec<ValidatorExtend>), Error> {
        let previous_number = block_number
            .checked_sub(1u64.into())
            .expect("bad block_number");
        let next_number = block_number
            .checked_add(1u64.into())
            .expect("bad block_number");

        let block = self
            .rpc_client
            .get_block_by_id(block_number.into())
            .await?
            .ok_or_else(|| Error::other_error(format!("failed to get block {block_number}")))?;
        let state_root = self
            .rpc_client
            .get_block_by_id(previous_number.into())
            .await?
            .ok_or_else(|| Error::other_error(format!("failed to get block {previous_number}")))?
            .header
            .state_root;
        let proof = loop {
            match self.rpc_client.get_proof_by_id(next_number.into()).await? {
                None => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Some(p) => break p,
            }
        };
        let validators = self
            .rpc_client
            .get_current_metadata()
            .await?
            .verifier_list
            .into_iter()
            .map(|v| ValidatorExtend {
                bls_pub_key: v.bls_pub_key.clone(),
                pub_key: v.pub_key.clone(),
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
            .block_on(async { Ok($self.contract()?.$method(msg.clone()).send().await?.await?) })
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
                    Ok(self
                        .contract()?
                        .recv_packet(msg.into())
                        .send()
                        .await?
                        .await?)
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
            debug!(
                "Axon received '{}' with events of {}",
                message.type_url.as_str(),
                events.len()
            );
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
            Error::send_tx("not find right event from Axon transaction receipt.".to_owned())
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
        let mut ibc_cache = self.ibc_cache.write().unwrap();
        cache_ics_tx_hash_with_event(&mut ibc_cache, event.clone(), tx_hash);
        tracing::info!(
            "{} transaciton {} committed to {}",
            event.event_type().as_str(),
            hex::encode(tx_hash),
            self.id()
        );
        Ok(IbcEventWithHeight {
            event,
            height,
            tx_hash,
        })
    }
}

fn cache_ics_tx_hash<T: Into<[u8; 32]>>(
    ibc_cache: &mut IBCInfoCache,
    cached_status: CacheTxHashStatus,
    tx_hash: T,
) {
    let hash: [u8; 32] = tx_hash.into();
    match cached_status {
        CacheTxHashStatus::Connection(conn_id) => {
            ibc_cache.conn_tx_hash.insert(conn_id, hash.into());
        }
        CacheTxHashStatus::Channel(chan_id, port_id) => {
            ibc_cache
                .chan_tx_hash
                .insert((chan_id, port_id), hash.into());
        }
        CacheTxHashStatus::Packet(chan_id, port_id, sequence) => {
            ibc_cache
                .packet_tx_hash
                .insert((chan_id, port_id, sequence), hash.into());
        }
    }
}

pub(crate) fn cache_ics_tx_hash_with_event<T: Into<[u8; 32]>>(
    ibc_cache: &mut IBCInfoCache,
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
        cache_ics_tx_hash(ibc_cache, tx_hash_status, tx_hash);
    }
}
