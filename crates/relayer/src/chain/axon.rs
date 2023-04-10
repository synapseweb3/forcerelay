#![allow(dead_code, unused_variables, unused_imports)]

use std::{
    sync::{self, Arc},
    thread,
};

use crate::{
    account::Balance,
    chain::{axon::contract::HeightData, requests::QueryHeight},
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, ChainConfig},
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::axon::LightClient as AxonLightClient,
    misbehaviour::MisbehaviourEvidence,
};
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
        ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd},
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd},
            packet::Sequence,
        },
        ics23_commitment::{commitment::CommitmentPrefix, merkle::MerkleProof},
        ics24_host::identifier::{self, ChainId, ChannelId, ClientId, PortId},
    },
    signer::Signer,
    Height,
};
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
    keybase: KeyRing<Secp256k1KeyPair>,
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
        })
    }

    fn shutdown(self) -> Result<(), Error> {
        tracing::debug!("runtime of eth chain endpoint shutdown");
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

    // Can't implement
    fn query_balance(&self, key_name: Option<&str>, denom: Option<&str>) -> Result<Balance, Error> {
        // NOTE: or return a 0 balance?
        todo!()
    }

    // Can't implement
    fn query_all_balances(&self, key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        // NOTE: or return a 0 balance vector?
        todo!()
    }

    // Can't implement
    fn query_denom_trace(&self, hash: String) -> Result<DenomTrace, Error> {
        todo!()
    }

    // Can't implement?
    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        todo!()
    }

    // Can't implement?
    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        todo!()
    }

    // Can't implement. Need access store
    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        todo!()
    }

    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support client state query with proof".to_string(),
            ));
        }
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support client state query in specific height".to_string(),
            ));
        }
        let (client_state, _) = self
            .rt
            .block_on(
                self.contract
                    .get_client_state(request.client_id.to_string())
                    .call(),
            )
            .map_err(map_contract_error)?;

        let client_state: AnyClientState =
            todo!("Type conversion. How to get specific consensus state from bytes?");
        return Ok((client_state, None));
    }

    fn query_consensus_state(
        &self,
        request: QueryConsensusStateRequest,
        include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support consensus state query with proof".to_string(),
            ));
        }
        if matches!(request.query_height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support consensus state query in specific height".to_string(),
            ));
        }
        let (consensus_state, _) = self
            .rt
            .block_on(
                self.contract
                    .get_consensus_state(
                        request.client_id.to_string(),
                        HeightData {
                            revision_number: request.consensus_height.revision_number(),
                            revision_height: request.consensus_height.revision_number(),
                        },
                    )
                    .call(),
            )
            .map_err(map_contract_error)?;
        let consensus_state: AnyConsensusState =
            todo!("Type conversion. How to get specific consensus state from bytes?");
        return Ok((consensus_state, None));
    }

    // NOTE: can't implement. How to get all consensus state?
    fn query_consensus_state_heights(
        &self,
        request: QueryConsensusStateHeightsRequest,
    ) -> Result<Vec<Height>, Error> {
        todo!()
    }

    // NOTE: what's this?
    fn query_upgraded_client_state(
        &self,
        request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        todo!()
    }

    // NOTE: what's this?
    fn query_upgraded_consensus_state(
        &self,
        request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        todo!()
    }

    // NOTE: Can't implement. Need access store
    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        todo!()
    }

    // NOTE: Can't implement. Need access store
    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<identifier::ConnectionId>, Error> {
        todo!()
    }

    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support connection query with proof".to_string(),
            ));
        }
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support connection query in specific height".to_string(),
            ));
        }
        let (connection_end, _) = self
            .rt
            .block_on(
                self.contract
                    .get_connection(request.connection_id.to_string())
                    .call(),
            )
            .map_err(map_contract_error)?;
        let connection_end: ConnectionEnd =
            todo!("Type conversion. Fields checked and conversion is feasible.");
        // Only be able to return None for MerkleProof as for now contract
        // does not provide query_proof methods in light client interface
        return Ok((connection_end, None));
    }

    // NOTE: Can't implement. Need access store
    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        todo!()
    }

    // NOTE: Can't implement. Need access store
    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        todo!()
    }

    fn query_channel(
        &self,
        request: QueryChannelRequest,
        include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support channel query with proof".to_string(),
            ));
        }
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support channel query in specific height".to_string(),
            ));
        }
        let (channel_end, _) = self
            .rt
            .block_on(
                self.contract
                    .get_channel(request.port_id.to_string(), request.channel_id.to_string())
                    .call(),
            )
            .map_err(map_contract_error)?;
        let channel_end: ChannelEnd =
            todo!("Type conversion. Fields checked and conversion is feasible.");
        return Ok((channel_end, None));
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        let query_channel_request = QueryChannelRequest {
            port_id: request.port_id,
            channel_id: request.channel_id,
            height: requests::QueryHeight::Latest,
        };
        let (channel_end, _) = self.query_channel(query_channel_request, IncludeProof::No)?;
        let connection_id = if let Some(connection_id) = channel_end.connection_hops.get(0) {
            connection_id
        } else {
            return Ok(None);
        };

        let query_connection_request = QueryConnectionRequest {
            connection_id: channel_end.connection_hops.get(0).unwrap().clone(),
            height: QueryHeight::Latest,
        };
        let (connection_end, _) =
            self.query_connection(query_connection_request, IncludeProof::No)?;
        let client_id = connection_end.client_id();
        let query_client_state = QueryClientStateRequest {
            client_id: client_id.clone(),
            height: QueryHeight::Latest,
        };
        let (client_state, _) = self.query_client_state(query_client_state, IncludeProof::No)?;
        return Ok(Some(IdentifiedAnyClientState {
            client_id: client_id.clone(),
            client_state,
        }));
    }

    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support packet commitment query with proof".to_string(),
            ));
        }
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support packet commitment query in specific height".to_string(),
            ));
        }
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
            .map_err(map_contract_error)?;
        let commitment = commitment.to_vec();
        return Ok((commitment, None));
    }

    // NOTE: Can't implement. Need access store
    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        todo!()
    }

    // NOTE: Can't implement. Need design packet structure in contract.
    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        todo!()
    }

    // NOTE: Can't implement. How to implement?
    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        todo!()
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        if matches!(IncludeProof::Yes, include_proof) {
            return Err(Error::other_error(
                "can't support packet acknowledgement query with proof".to_string(),
            ));
        }
        if matches!(request.height, QueryHeight::Specific(_)) {
            return Err(Error::other_error(
                "can't support packet commitment query in specific height".to_string(),
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
            .map_err(map_contract_error)?;
        let commitment = commitment.to_vec();
        return Ok((commitment, None));
    }

    // NOTE: Can't implement. Need access store
    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        todo!()
    }

    // NOTE: Can't implement. Need access store
    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        todo!()
    }

    // NOTE: Can't implement. Need access store
    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        todo!()
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
}

impl AxonChain {
    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        crate::time!("axon_init_event_monitor");
        let (create_receiver, header_receiver) = self.light_client.subscribe();
        // TODO: configure URLs
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
}

fn map_contract_error(contract_err: ethers_contract::ContractError<ContractProvider>) -> Error {
    todo!()
}
