#![allow(dead_code, unused_variables, unused_imports)]

use std::{
    sync::{self, Arc},
    thread,
};

use crate::{
    account::Balance,
    chain::{axon::contract::HeightData, requests::QueryHeight},
    client_state::{AnyClientState, IdentifiedAnyClientState},
    config::{axon::AxonChainConfig, filter::port, ChainConfig},
    consensus_state::AnyConsensusState,
    denom::DenomTrace,
    error::Error,
    event::{monitor::TxMonitorCmd, IbcEventWithHeight},
    keyring::{KeyRing, Secp256k1KeyPair},
    light_client::axon::LightClient as AxonLightClient,
    misbehaviour::MisbehaviourEvidence,
    util::collate::collate,
};
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
        ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd},
        ics04_channel::{
            channel::{ChannelEnd, IdentifiedChannelEnd},
            packet::Sequence,
        },
        ics23_commitment::{commitment::CommitmentPrefix, merkle::MerkleProof},
        ics24_host::identifier::{self, ChainId, ChannelId, ClientId, ConnectionId, PortId},
    },
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
            .map_err(map_contract_error)?;
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
            .map_err(map_contract_error)?;

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
            .map_err(map_contract_error)?;
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
            .map_err(map_contract_error)?;
        let heights = heights
            .iter()
            .map(|height| Height::new(height.revision_number, height.revision_height))
            .collect::<Result<Vec<Height>, _>>()
            .map_err(|_| Error::invalid_height_no_source())?;
        return Ok(heights);
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
            .map_err(map_contract_error)?;
        let connections = connections
            .iter()
            .map(to_identified_connection_end)
            .collect::<Result<Vec<_>, Error>>()?;
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
            .map_err(map_contract_error)?;
        let connection_ids = connection_ids
            .iter()
            .map(to_any_connection_id)
            .collect::<Result<Vec<_>, Error>>()?;
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
            .map_err(map_contract_error)?;
        let connection_end = to_connection_end(&connection_end)?;
        return Ok((connection_end, None));
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
            .map_err(map_contract_error)?;
        let channels = channels
            .iter()
            .map(to_identified_channel_end)
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(channels)
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        let channels: Vec<_> = self
            .rt
            .block_on(self.contract.get_channels().call())
            .map_err(map_contract_error)?;
        let channels = channels
            .iter()
            .map(to_identified_channel_end)
            .collect::<Result<Vec<_>, Error>>()?;
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
            .map_err(map_contract_error)?;
        let channel_end = to_channel_end(&channel_end)?;
        return Ok((channel_end, None));
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
            .map_err(map_contract_error)?;

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
            .map_err(map_contract_error)?;
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
            .map_err(map_contract_error)?;

        let commitment_sequences = commitment_sequences
            .iter()
            .map(|seq| seq.clone().into())
            .collect();
        let height: Height = todo!("how to get height?");
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
            .map_err(map_contract_error)?;
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
                .map_err(map_contract_error)?;
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
            .map_err(map_contract_error)?;
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
                            seq.clone().into(),
                        )
                        .call(),
                )
                .map_err(map_contract_error)?;
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
                            seq.clone().into(),
                        )
                        .call(),
                )
                .map_err(map_contract_error)?;
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
            .map_err(map_contract_error)?;
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

fn to_identified_any_client_state(
    client_state: &ethers::core::types::Bytes,
) -> Result<IdentifiedAnyClientState, Error> {
    todo!("Type conversion. How to get specific consensus state from bytes?")
}

fn to_any_client_state(client_state: &ethers::core::types::Bytes) -> Result<AnyClientState, Error> {
    todo!("Type conversion. How to get specific consensus state from bytes?");
}

fn to_identified_channel_end(
    client_state: &contract::IdentifiedChannelData,
) -> Result<IdentifiedChannelEnd, Error> {
    todo!("Type conversion");
}

fn to_channel_end(client_state: &contract::ChannelData) -> Result<ChannelEnd, Error> {
    todo!("Type conversion");
}

fn to_connection_end(connection_end: &contract::ConnectionEndData) -> Result<ConnectionEnd, Error> {
    todo!("Type conversion");
}

fn to_identified_connection_end(
    connection_end: &contract::IdentifiedConnectionEndData,
) -> Result<IdentifiedConnectionEnd, Error> {
    todo!("Type conversion");
}

fn to_any_consensus_state(
    consensus_state: &ethers::core::types::Bytes,
) -> Result<AnyConsensusState, Error> {
    todo!("Type conversion.");
}

fn to_any_connection_id(connection_id: &String) -> Result<ConnectionId, Error> {
    todo!("Type conversion.");
}
