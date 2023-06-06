use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::account::Balance;
use crate::chain::ckb::prelude::{CellSearcher, CkbReader, CkbWriter, TxCompleter};
use crate::chain::ckb4ibc::extractor::extract_channel_end_from_tx;
use crate::chain::endpoint::ChainEndpoint;
use crate::client_state::{AnyClientState, IdentifiedAnyClientState};
use crate::config::ckb4ibc::ChainConfig as Ckb4IbcChainConfig;
use crate::config::ChainConfig;
use crate::consensus_state::AnyConsensusState;
use crate::denom::DenomTrace;
use crate::error::Error;
use crate::event::monitor::TxMonitorCmd;
use crate::event::IbcEventWithHeight;
use crate::keyring::{KeyRing, Secp256k1KeyPair};
use crate::misbehaviour::MisbehaviourEvidence;

use ckb_ics_axon::handler::{IbcChannel, IbcConnections, IbcPacket, PacketStatus};
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::{ChannelArgs, PacketArgs};
use ckb_jsonrpc_types::{JsonBytes, Status, TransactionView};
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_light_client::{ScriptType, SearchKey};
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::core::ScriptHashType;
use ckb_types::core::TransactionView as CoreTransactionView;
use ckb_types::molecule::prelude::Entity;
use ckb_types::packed::{CellInput, OutPoint, Script, WitnessArgs};
use ckb_types::prelude::{Builder, Pack};
use futures::TryFutureExt;
use ibc_proto::ibc::apps::fee::v1::{
    QueryIncentivizedPacketRequest, QueryIncentivizedPacketResponse,
};
use ibc_relayer_types::applications::ics31_icq::response::CrossChainQueryResponse;
use ibc_relayer_types::clients::ics07_ckb::{
    client_state::ClientState as CkbClientState,
    consensus_state::ConsensusState as CkbConsensusState, header::Header as CkbHeader,
    light_block::LightBlock as CkbLightBlock,
};
use ibc_relayer_types::core::ics02_client::events::UpdateClient;
use ibc_relayer_types::core::ics03_connection::connection::{
    ConnectionEnd, IdentifiedConnectionEnd,
};
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc_relayer_types::core::ics23_commitment::merkle::MerkleProof;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ChannelId, ConnectionId, PortId};
use ibc_relayer_types::signer::Signer;
use ibc_relayer_types::Height;
use semver::Version;
use std::sync::RwLock;
use tendermint_rpc::endpoint::broadcast::tx_sync::Response;
use tokio::runtime::Runtime;

use self::extractor::{extract_connections_from_tx, extract_ibc_packet_from_tx};
use self::message::{convert_msg_to_ckb_tx, Converter, MsgToTxConverter};
use self::monitor::Ckb4IbcEventMonitor;
use self::utils::{get_channel_idx, get_encoded_object, get_search_key};

use super::ckb::rpc_client::RpcClient;
use super::client::ClientSettings;
use super::cosmos::encode::key_pair_to_signer;
use super::endpoint::{ChainStatus, HealthCheck};
use super::handle::Subscription;
use super::requests::{
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
};
use super::tracking::TrackedMsgs;
use tokio::runtime::Runtime as TokioRuntime;

mod cache_set;
mod extractor;
pub mod message;
mod monitor;
mod utils;

pub struct Ckb4IbcChain {
    rt: Arc<TokioRuntime>,
    rpc_client: Arc<RpcClient>,
    config: Ckb4IbcChainConfig,
    keybase: KeyRing<Secp256k1KeyPair>,
    cached_network: RwLock<Option<NetworkType>>,

    tx_monitor_cmd: Option<TxMonitorCmd>,

    client_outpoint: OutPoint,
    channel_input_data: RefCell<HashMap<(ChannelId, PortId), CellInput>>,
    channel_cache: RefCell<HashMap<ChannelId, IbcChannel>>,
    connection_cache: RefCell<Option<(IbcConnections, CellInput)>>,
    packet_input_data: RefCell<HashMap<(ChannelId, PortId, Sequence), CellInput>>,

    cached_tx_assembler_address: RwLock<Option<Address>>,
}

impl Ckb4IbcChain {
    pub fn network(&self) -> Result<NetworkType, Error> {
        let cached_network_opt: Option<NetworkType> =
            *self.cached_network.read().map_err(Error::other)?;
        let network = if let Some(network) = cached_network_opt {
            network
        } else {
            let network = {
                let chain_info = self
                    .rt
                    .block_on(self.rpc_client.get_blockchain_info())
                    .map_err(|e| Error::rpc_response(e.to_string()))?;
                if chain_info.chain == "ckb" {
                    NetworkType::Mainnet
                } else if chain_info.chain == "ckb_testnet" {
                    NetworkType::Testnet
                } else {
                    NetworkType::Dev
                }
            };
            *self.cached_network.write().map_err(Error::other)? = Some(network);
            network
        };
        Ok(network)
    }

    pub fn tx_assembler_address(&self) -> Result<Address, Error> {
        let cached_address = self
            .cached_tx_assembler_address
            .read()
            .map_err(Error::other)?
            .clone();
        let address = if let Some(address) = cached_address {
            address
        } else {
            let network = self.network()?;
            let key: Secp256k1KeyPair = self
                .keybase
                .get_key(&self.config.key_name)
                .map_err(Error::key_base)?;
            let address_payload = AddressPayload::from_pubkey(&key.public_key);
            let address = Address::new(network, address_payload, true);
            *self
                .cached_tx_assembler_address
                .write()
                .map_err(Error::other)? = Some(address.clone());
            address
        };
        Ok(address)
    }

    pub fn get_converter(&self) -> Converter {
        Converter {
            channel_input_data: self.channel_input_data.borrow(),
            channel_cache: self.channel_cache.borrow(),
            config: &self.config,
            connection_cache: self.connection_cache.borrow(),
            client_outpoint: &self.client_outpoint,
            packet_input_data: self.packet_input_data.borrow(),
            packet_owner: Default::default(), // todo
        }
    }

    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        let (monitor, monitor_tx) = Ckb4IbcEventMonitor::new(
            self.rt.clone(),
            self.rpc_client.clone(),
            self.config.clone(),
        );
        std::thread::spawn(move || monitor.run());
        Ok(monitor_tx)
    }

    fn fetch_packet_cell_and_extract(
        &self,
        channel_id: &ChannelId,
        port_id: &PortId,
        sequence: Sequence,
    ) -> Result<(IbcPacket, CellInput), Error> {
        let script = Script::new_builder()
            .code_hash(self.get_converter().get_packet_code_hash())
            .hash_type(ScriptHashType::Type.into())
            .args(
                PacketArgs {
                    channel_id: get_channel_idx(channel_id)?,
                    port_id: port_id.as_str().as_bytes().try_into().unwrap(),
                    sequence: u64::from(sequence) as u16,
                    owner: Default::default(),
                }
                .get_search_args()
                .pack(),
            )
            .build();
        let search_key = get_search_key(script);
        let resp = self
            .rpc_client
            .fetch_live_cells(search_key, 1, None)
            .and_then(|resp| async move {
                let cell = resp
                    .objects
                    .into_iter()
                    .next()
                    .ok_or(Error::query(String::from("query packet")))?;
                let tx_hash = &cell.out_point.tx_hash;
                let tx_resp = self
                    .rpc_client
                    .get_transaction(tx_hash)
                    .await
                    .map_err(|_| Error::query("".to_string()))?
                    .ok_or(Error::query("".to_string()))?
                    .transaction
                    .unwrap();
                let tx = match tx_resp.inner {
                    ckb_jsonrpc_types::Either::Left(r) => r,
                    ckb_jsonrpc_types::Either::Right(json_bytes) => {
                        let bytes = json_bytes.as_bytes();
                        let tx: TransactionView = serde_json::from_slice(bytes).unwrap();
                        tx
                    }
                };
                let ibc_packet = extract_ibc_packet_from_tx(tx)?;
                let cell_input = CellInput::new_builder()
                    .previous_output(cell.out_point.into())
                    .build();
                Ok((ibc_packet, cell_input))
            });
        let result = self.rt.block_on(resp)?;
        Ok(result)
    }

    fn fetch_channel_cell_and_extract(
        &self,
        channel_id: ChannelId,
        port_id: PortId,
        open: bool,
    ) -> Result<ChannelEnd, Error> {
        let channel_code_hash = self.get_converter().get_channel_code_hash();
        let script = Script::new_builder()
            .code_hash(channel_code_hash)
            .args(
                ChannelArgs {
                    client_id: self.config.client_id,
                    open,
                    channel_id: get_channel_idx(&channel_id)?,
                    port_id: port_id.as_str().as_bytes().try_into().unwrap(),
                }
                .to_args()
                .pack(),
            )
            .hash_type(ScriptHashType::Type.into())
            .build();
        let search_key = get_search_key(script);
        let channel_end_future = self
            .rpc_client
            .fetch_live_cells(search_key, 1, None)
            .and_then(|resp| async move {
                let cell = resp.objects.first().ok_or(Error::query("".to_string()))?;
                let tx_hash = &cell.out_point.tx_hash;
                let tx_resp = self
                    .rpc_client
                    .get_transaction(tx_hash)
                    .await
                    .map_err(|_| Error::query("".to_string()))?
                    .ok_or(Error::query("".to_string()))?
                    .transaction
                    .unwrap();
                let tx = match tx_resp.inner {
                    ckb_jsonrpc_types::Either::Left(r) => r,
                    ckb_jsonrpc_types::Either::Right(json_bytes) => {
                        let bytes = json_bytes.as_bytes();
                        let tx: TransactionView = serde_json::from_slice(bytes).unwrap();
                        tx
                    }
                };
                let channel_end = extract_channel_end_from_tx(tx)?;
                let input = CellInput::new_builder()
                    .previous_output(
                        OutPoint::new_builder()
                            .tx_hash(tx_hash.pack())
                            .index(cell.tx_index.pack())
                            .build(),
                    )
                    .build();
                Ok((channel_end, input))
            });
        let ((channel_end, ibc_channel_end), cell_input) = self.rt.block_on(channel_end_future)?;

        let mut data = self.channel_input_data.borrow_mut();
        data.insert(
            (channel_end.channel_id.clone(), channel_end.port_id),
            cell_input,
        );
        let mut cache = self.channel_cache.borrow_mut();
        cache.insert(channel_end.channel_id, ibc_channel_end);
        Ok(channel_end.channel_end)
    }

    fn clear_cache(&mut self) {
        let channel_data = self.channel_input_data.get_mut();
        channel_data.clear();

        let channel_cache = self.channel_cache.get_mut();
        channel_cache.clear();

        let packet_data = self.packet_input_data.get_mut();
        packet_data.clear();

        self.connection_cache.swap(&RefCell::new(None));
    }

    fn query_connection(
        &self,
    ) -> Result<(Vec<IdentifiedConnectionEnd>, IbcConnections, CellInput), Error> {
        let script = Script::new_builder()
            .code_hash(self.get_converter().get_connection_code_hash())
            .args(self.config.client_id.to_vec().pack())
            .hash_type(ScriptHashType::Type.into())
            .build();
        let search_key = SearchKey {
            script: script.into(),
            script_type: ScriptType::Lock,
            filter: None,
            with_data: None,
            group_by_transaction: None,
        };
        let cells_rpc_result = self
            .rpc_client
            .fetch_live_cells(search_key, 1, None)
            .and_then(|cells| async {
                let cell = cells
                    .objects
                    .into_iter()
                    .next()
                    .ok_or(Error::query("get ibc connection cell failed 1".to_string()))?;
                let tx_resp = self
                    .rpc_client
                    .get_transaction(&cell.out_point.tx_hash)
                    .await?;
                Ok((
                    tx_resp,
                    CellInput::new_builder()
                        .previous_output(cell.out_point.into())
                        .build(),
                ))
            });
        let (transaction, cell_input) = self.rt.block_on(cells_rpc_result)?;
        let tx = transaction
            .ok_or(Error::query("get ibc connection cell failed 2".to_string()))?
            .transaction
            .ok_or(Error::query("get ibc connection cell failed 3".to_string()))?;
        let tx = match tx.inner {
            ckb_jsonrpc_types::Either::Left(r) => r,
            ckb_jsonrpc_types::Either::Right(json_bytes) => {
                let bytes = json_bytes.as_bytes();
                let tx: TransactionView = serde_json::from_slice(bytes).unwrap();
                tx
            }
        };
        let (connections, ibc_connection) = extract_connections_from_tx(tx)?;
        Ok((connections, ibc_connection, cell_input))
    }

    pub fn complete_tx_with_secp256k1_change_and_envelope(
        &self,
        tx: CoreTransactionView,
        input_capacity: u64,
        envelope: Envelope,
    ) -> Result<CoreTransactionView, Error> {
        let fee_rate = 3000;
        let address = self.tx_assembler_address()?;
        let tx = self.rpc_client.complete_tx_with_secp256k1_change(
            tx,
            &address,
            input_capacity,
            fee_rate,
        );
        let (result, _) = self.rt.block_on(tx)?;
        let witness = WitnessArgs::new_builder()
            .output_type(get_encoded_object(envelope).witness)
            .build()
            .as_bytes()
            .pack();
        let result = result.as_advanced_builder().witness(witness).build();
        Ok(result)
    }
}

impl ChainEndpoint for Ckb4IbcChain {
    type LightBlock = CkbLightBlock;

    type Header = CkbHeader;

    type ConsensusState = CkbConsensusState;

    type ClientState = CkbClientState;

    type SigningKeyPair = Secp256k1KeyPair;

    fn config(&self) -> ChainConfig {
        ChainConfig::Ckb4Ibc(self.config.clone())
    }

    fn bootstrap(config: ChainConfig, rt: Arc<Runtime>) -> Result<Self, Error> {
        let config: Ckb4IbcChainConfig = config.try_into()?;
        let rpc_client = Arc::new(RpcClient::new(&config.ckb_rpc, &config.ckb_indexer_rpc));
        let client_cell = rt.block_on(rpc_client.search_cell_by_typescript(
            &TYPE_ID_CODE_HASH.pack(),
            &config.client_type_args.as_bytes().to_owned(),
        ))?;
        if client_cell.is_none() {
            return Err(Error::other_error(
                "invalid `lightclient_contract_typeargs` option".to_owned(),
            ));
        }
        let keybase =
            KeyRing::new(Default::default(), "ckb", &config.id).map_err(Error::key_base)?;
        let client_outpoint = client_cell.unwrap().out_point;
        let chain = Ckb4IbcChain {
            rt,
            rpc_client,
            config,
            keybase,
            cached_network: RwLock::new(None),
            tx_monitor_cmd: None,
            client_outpoint,
            channel_input_data: RefCell::new(HashMap::new()),
            channel_cache: RefCell::new(HashMap::new()),
            connection_cache: RefCell::new(None),
            packet_input_data: RefCell::new(HashMap::new()),
            cached_tx_assembler_address: RwLock::new(None),
        };
        Ok(chain)
    }

    fn shutdown(self) -> Result<(), Error> {
        if let Some(monitor_tx) = self.tx_monitor_cmd {
            monitor_tx.shutdown().map_err(Error::event_monitor)?;
        }

        Ok(())
    }

    fn health_check(&self) -> Result<HealthCheck, Error> {
        Ok(HealthCheck::Healthy)
    }

    fn subscribe(&mut self) -> Result<Subscription, Error> {
        let tx_monitor_cmd = match &self.tx_monitor_cmd {
            Some(result) => result,
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

    fn ibc_version(&self) -> Result<Option<Version>, Error> {
        Ok(None)
    }

    fn send_messages_and_wait_commit(
        &mut self,
        tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        let mut txs = Vec::new();
        let converter = self.get_converter();
        for msg in tracked_msgs.msgs {
            let (tx, envelope, input_capacity) = convert_msg_to_ckb_tx(msg, &converter)?;
            if let Ok(tx) =
                self.complete_tx_with_secp256k1_change_and_envelope(tx, input_capacity, envelope)
            {
                txs.push(tx);
            }
        }
        let resps = txs.into_iter().map(|tx| {
            let tx: TransactionView = tx.into();
            // sign here
            self.rpc_client.send_transaction(&tx.inner, None)
        });
        let has_error = self
            .rt
            .block_on(futures::future::join_all(resps))
            .into_iter()
            .any(|r| r.is_err());

        drop(converter);
        self.clear_cache();

        if has_error {
            Err(Error::send_tx("todo".into()))
        } else {
            Ok(vec![])
        }
    }

    fn send_messages_and_wait_check_tx(
        &mut self,
        _tracked_msgs: TrackedMsgs,
    ) -> Result<Vec<Response>, Error> {
        todo!()
    }

    fn verify_header(
        &mut self,
        _trusted: Height,
        _target: Height,
        _client_state: &AnyClientState,
    ) -> Result<Self::LightBlock, Error> {
        todo!()
    }

    fn check_misbehaviour(
        &mut self,
        _update: &UpdateClient,
        _client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        Ok(None)
    }

    fn query_balance(
        &self,
        _key_name: Option<&str>,
        _denom: Option<&str>,
    ) -> Result<Balance, Error> {
        todo!()
    }

    fn query_all_balances(&self, _key_name: Option<&str>) -> Result<Vec<Balance>, Error> {
        todo!()
    }

    fn query_denom_trace(&self, _hash: String) -> Result<DenomTrace, Error> {
        todo!()
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        todo!()
    }

    fn query_application_status(&self) -> Result<ChainStatus, Error> {
        todo!()
    }

    fn query_clients(
        &self,
        _request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        todo!()
    }

    fn query_client_state(
        &self,
        _request: QueryClientStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        todo!()
    }

    fn query_consensus_state(
        &self,
        _request: QueryConsensusStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        todo!()
    }

    fn query_consensus_state_heights(
        &self,
        _request: QueryConsensusStateHeightsRequest,
    ) -> Result<Vec<Height>, Error> {
        Ok(vec![])
    }

    fn query_upgraded_client_state(
        &self,
        _request: QueryUpgradedClientStateRequest,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        todo!()
    }

    fn query_upgraded_consensus_state(
        &self,
        _request: QueryUpgradedConsensusStateRequest,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        todo!()
    }

    fn query_connections(
        &self,
        _request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        let (result, _, _) = self.query_connection()?;
        Ok(result)
    }

    fn query_client_connections(
        &self,
        _request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        let (result, _, _) = self.query_connection()?;
        Ok(result.into_iter().map(|c| c.id().clone()).collect())
    }

    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        _include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        let (connections, ibc_connection, cell_input) = self.query_connection()?;
        let result = std::cell::RefCell::new(Some((ibc_connection, cell_input)));
        self.connection_cache.swap(&result);
        let err = || Error::ckb_conn_id_invalid(request.connection_id.as_str().to_string());
        let idx = request
            .connection_id
            .as_str()
            .parse::<usize>()
            .map_err(|_| err())?;
        let connection_end = connections.into_iter().nth(idx - 1).ok_or(err())?;
        Ok((connection_end.connection_end, None))
    }

    fn query_connection_channels(
        &self,
        _request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        self.query_channels(QueryChannelsRequest { pagination: None })
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        let channel_code_hash = self.get_converter().get_channel_code_hash();
        let script = Script::new_builder()
            .code_hash(channel_code_hash)
            .args("".pack())
            .hash_type(ScriptHashType::Type.into())
            .build();
        let search_key = get_search_key(script);
        let (limit, index) = {
            if let Some(pagination) = request.pagination {
                (pagination.limit as u32, pagination.offset as u32)
            } else {
                (100, 0)
            }
        };
        let json_bytes = JsonBytes::from_vec(index.to_be_bytes().to_vec());
        let cursor = Some(json_bytes);
        let cells_rpc_result = self.rpc_client.fetch_live_cells(search_key, limit, cursor);
        let txs_rpc_result = self
            .rt
            .block_on(cells_rpc_result)?
            .objects
            .into_iter()
            .map(|cell| self.rpc_client.get_transaction(&cell.out_point.tx_hash));
        let channel_ends = self
            .rt
            .block_on(futures::future::join_all(txs_rpc_result))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|resp| resp.tx_status.status == Status::Committed && resp.transaction.is_some())
            .flat_map(|tx| {
                let tx_resp = tx.transaction.unwrap();
                let tx = match tx_resp.inner {
                    ckb_jsonrpc_types::Either::Left(r) => r,
                    ckb_jsonrpc_types::Either::Right(json_bytes) => {
                        let bytes = json_bytes.as_bytes();
                        let tx: TransactionView = serde_json::from_slice(bytes).unwrap();
                        tx
                    }
                };
                extract_channel_end_from_tx(tx)
            })
            .map(|e| e.0)
            .collect();
        Ok(channel_ends)
    }

    fn query_channel(
        &self,
        request: QueryChannelRequest,
        _include_proof: IncludeProof,
    ) -> Result<(ChannelEnd, Option<MerkleProof>), Error> {
        if let Ok(r) = self.fetch_channel_cell_and_extract(
            request.channel_id.clone(),
            request.port_id.clone(),
            false,
        ) {
            Ok((r, None))
        } else {
            let r =
                self.fetch_channel_cell_and_extract(request.channel_id, request.port_id, true)?;
            Ok((r, None))
        }
    }

    fn query_channel_client_state(
        &self,
        _request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        Ok(None)
    }

    fn query_packet_commitment(
        &self,
        request: QueryPacketCommitmentRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let (ibc_packet, _) = self.fetch_packet_cell_and_extract(
            &request.channel_id,
            &request.port_id,
            request.sequence,
        )?;
        if ibc_packet.status != PacketStatus::Send {
            Ok((vec![], None))
        } else {
            Ok((
                PacketArgs {
                    channel_id: get_channel_idx(&request.channel_id)?,
                    port_id: ibc_packet
                        .packet
                        .source_port_id
                        .as_bytes()
                        .try_into()
                        .unwrap(),
                    sequence: ibc_packet.packet.sequence,
                    owner: Default::default(),
                }
                .get_search_args(),
                None,
            ))
        }
    }

    fn query_packet_commitments(
        &self,
        _request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        todo!()
    }

    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let (ibc_packet, _) = self.fetch_packet_cell_and_extract(
            &request.channel_id,
            &request.port_id,
            request.sequence,
        )?;
        if ibc_packet.status != PacketStatus::Recv {
            Ok((vec![], None))
        } else {
            Ok((
                PacketArgs {
                    channel_id: get_channel_idx(&request.channel_id)?,
                    port_id: ibc_packet
                        .packet
                        .source_port_id
                        .as_bytes()
                        .try_into()
                        .unwrap(),
                    sequence: ibc_packet.packet.sequence,
                    owner: Default::default(),
                }
                .get_search_args(),
                None,
            ))
        }
    }

    fn query_unreceived_packets(
        &self,
        _request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        todo!()
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let (ibc_packet, _) = self.fetch_packet_cell_and_extract(
            &request.channel_id,
            &request.port_id,
            request.sequence,
        )?;
        if ibc_packet.status != PacketStatus::InboxAck {
            Ok((vec![], None))
        } else {
            Ok((ibc_packet.tx_hash.unwrap().as_bytes().to_vec(), None))
        }
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        let port_id = request.port_id;
        let channel_id = request.channel_id;
        let result = request
            .packet_commitment_sequences
            .into_iter()
            .flat_map(|seq| self.fetch_packet_cell_and_extract(&channel_id, &port_id, seq))
            .filter(|(packet, _)| packet.status == PacketStatus::InboxAck)
            .map(|(p, _)| Sequence::from(p.packet.sequence as u64))
            .collect::<Vec<_>>();
        Ok((result, Height::new(u64::MAX, u64::MAX).unwrap()))
    }

    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let port_id = request.port_id;
        let channel_id = request.channel_id;
        let mut data = self.packet_input_data.borrow_mut();
        let result = request
            .packet_ack_sequences
            .into_iter()
            .flat_map(|seq| self.fetch_packet_cell_and_extract(&channel_id, &port_id, seq))
            .filter(|(packet, _)| packet.status == PacketStatus::Send)
            .map(|(p, cell_input)| {
                let seq = Sequence::from(p.packet.sequence as u64);
                data.insert((channel_id.clone(), port_id.clone(), seq), cell_input);
                seq
            })
            .collect::<Vec<_>>();
        Ok(result)
    }

    fn query_next_sequence_receive(
        &self,
        _request: QueryNextSequenceReceiveRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        todo!()
    }

    fn query_txs(&self, _request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        todo!()
    }

    fn query_packet_events(
        &self,
        _request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        todo!()
    }

    fn query_host_consensus_state(
        &self,
        _request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        todo!()
    }

    fn build_client_state(
        &self,
        _height: Height,
        _settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        todo!()
    }

    fn build_consensus_state(
        &self,
        _light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error> {
        todo!()
    }

    fn build_header(
        &mut self,
        _trusted_height: Height,
        _target_height: Height,
        _client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        todo!()
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        _channel_id: &ChannelId,
        _port_id: &PortId,
        _counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        todo!()
    }

    fn cross_chain_query(
        &self,
        _requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        todo!()
    }

    fn query_incentivized_packet(
        &self,
        _request: QueryIncentivizedPacketRequest,
    ) -> Result<QueryIncentivizedPacketResponse, Error> {
        todo!()
    }

    fn id(&self) -> ChainId {
        self.config().id().clone()
    }
}
