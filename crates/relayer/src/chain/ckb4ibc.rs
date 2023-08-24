use std::cell::RefCell;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::account::Balance;
use crate::chain::ckb::prelude::{CellSearcher, CkbReader, CkbWriter, TxCompleter};
use crate::chain::ckb4ibc::extractor::extract_channel_end_from_tx;
use crate::chain::ckb4ibc::utils::{get_connection_index_by_id, get_connection_search_key};
use crate::chain::endpoint::ChainEndpoint;
use crate::client_state::{AnyClientState, IdentifiedAnyClientState};
use crate::config::ckb4ibc::{ChainConfig as Ckb4IbcChainConfig, LightClientItem};
use crate::config::ChainConfig;
use crate::connection::ConnectionMsgType;
use crate::consensus_state::AnyConsensusState;
use crate::denom::DenomTrace;
use crate::error::Error;
use crate::event::monitor::TxMonitorCmd;
use crate::event::IbcEventWithHeight;
use crate::keyring::{KeyRing, Secp256k1KeyPair};
use crate::misbehaviour::MisbehaviourEvidence;

use ckb_ics_axon::handler::{IbcChannel, IbcConnections, IbcPacket, PacketStatus};
use ckb_ics_axon::message::Envelope;
use ckb_ics_axon::object::Ordering;
use ckb_ics_axon::{ChannelArgs, PacketArgs};
use ckb_jsonrpc_types::{JsonBytes, Status, TransactionView};
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::traits::SecpCkbRawKeySigner;
use ckb_sdk::unlock::{ScriptSigner, SecpSighashScriptSigner};
use ckb_sdk::{Address, AddressPayload, NetworkType, ScriptGroup, ScriptGroupType};
use ckb_types::core::ScriptHashType;
use ckb_types::core::TransactionView as CoreTransactionView;
use ckb_types::molecule::prelude::Entity;
use ckb_types::packed::{CellInput, OutPoint, Script, WitnessArgs};
use ckb_types::prelude::{Builder, Pack, Unpack};
use futures::TryFutureExt;
use ibc_proto::ibc::apps::fee::v1::{
    QueryIncentivizedPacketRequest, QueryIncentivizedPacketResponse,
};
use ibc_relayer_types::applications::ics31_icq::response::CrossChainQueryResponse;
use ibc_relayer_types::clients::ics07_ckb::{
    client_state::CkbClientState, consensus_state::CkbConsensusState, header::CkbHeader,
    light_block::CkbLightBlock,
};
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::error::Error as ClientError;
use ibc_relayer_types::core::ics02_client::events::UpdateClient;
use ibc_relayer_types::core::ics03_connection::connection::{
    ConnectionEnd, IdentifiedConnectionEnd,
};
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc_relayer_types::core::ics04_channel::packet::{PacketMsgType, Sequence};
use ibc_relayer_types::core::ics23_commitment::commitment::CommitmentPrefix;
use ibc_relayer_types::core::ics23_commitment::merkle::MerkleProof;
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ClientId, ConnectionId, PortId,
};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::proofs::Proofs;
use ibc_relayer_types::signer::Signer;
use ibc_relayer_types::timestamp::Timestamp;
use ibc_relayer_types::Height;
use rlp::Encodable;
use semver::Version;
use std::sync::RwLock;
use tendermint_rpc::endpoint::broadcast::tx_sync::Response;
use tokio::runtime::Runtime;
use tracing::log::{info, warn};

use self::extractor::{extract_connections_from_tx, extract_ibc_packet_from_tx};
use self::message::{convert_msg_to_ckb_tx, CkbTxInfo, Converter, MsgToTxConverter};
use self::monitor::Ckb4IbcEventMonitor;
use self::utils::{
    convert_port_id_to_array, get_channel_number, get_dummy_merkle_proof, get_encoded_object,
    get_search_key, get_search_key_with_sudt,
};

use super::ckb::rpc_client::RpcClient;
use super::ckb::utils::wait_ckb_transaction_committed;
use super::client::ClientSettings;
use super::cosmos::encode::key_pair_to_signer;
use super::endpoint::{ChainStatus, HealthCheck};
use super::handle::Subscription;
use super::requests::{
    CrossChainQueryRequest, IncludeProof, QueryChannelClientStateRequest, QueryChannelRequest,
    QueryChannelsRequest, QueryClientConnectionsRequest, QueryClientStateRequest,
    QueryClientStatesRequest, QueryConnectionChannelsRequest, QueryConnectionRequest,
    QueryConnectionsRequest, QueryConsensusStateHeightsRequest, QueryConsensusStateRequest,
    QueryHeight, QueryHostConsensusStateRequest, QueryNextSequenceReceiveRequest,
    QueryPacketAcknowledgementRequest, QueryPacketAcknowledgementsRequest,
    QueryPacketCommitmentRequest, QueryPacketCommitmentsRequest, QueryPacketEventDataRequest,
    QueryPacketReceiptRequest, QueryTxRequest, QueryUnreceivedAcksRequest,
    QueryUnreceivedPacketsRequest, QueryUpgradedClientStateRequest,
    QueryUpgradedConsensusStateRequest,
};
use super::tracking::TrackedMsgs;
use tokio::runtime::Runtime as TokioRuntime;

mod cache_set;
pub mod extractor;
pub mod message;
mod monitor;
pub mod utils;

pub use utils::keccak256;

pub struct Ckb4IbcChain {
    rt: Arc<TokioRuntime>,
    rpc_client: Arc<RpcClient>,
    config: Ckb4IbcChainConfig,
    keybase: KeyRing<Secp256k1KeyPair>,
    cached_network: RwLock<Option<NetworkType>>,

    tx_monitor_cmd: Option<TxMonitorCmd>,

    connection_outpoint: OutPoint,
    channel_outpoint: OutPoint,
    packet_outpoint: OutPoint,

    counterparty_client_type: tokio::sync::watch::Sender<Option<ClientType>>,

    client_outpoints: RefCell<HashMap<ClientType, OutPoint>>,
    channel_input_data: RefCell<HashMap<(ChannelId, PortId), CellInput>>,
    channel_cache: RefCell<HashMap<ChannelId, IbcChannel>>,
    #[allow(clippy::type_complexity)]
    connection_cache:
        RefCell<HashMap<ClientType, (IbcConnections, CellInput, Vec<IdentifiedConnectionEnd>)>>,
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

    pub fn get_converter(&self) -> Result<Converter, Error> {
        if self.connection_cache.borrow().is_empty() {
            self.query_connection_and_cache()?;
        }
        Ok(Converter {
            channel_input_data: self.channel_input_data.borrow(),
            channel_cache: self.channel_cache.borrow(),
            config: &self.config,
            connection_cache: self.connection_cache.borrow(),
            client_outpoints: self.client_outpoints.borrow(),
            packet_input_data: self.packet_input_data.borrow(),
            chan_contract_outpoint: &self.channel_outpoint,
            packet_contract_outpoint: &self.packet_outpoint,
            conn_contract_outpoint: &self.connection_outpoint,
            commitment_prefix: self.query_commitment_prefix()?,
        })
    }

    fn init_event_monitor(&mut self) -> Result<TxMonitorCmd, Error> {
        let (monitor, monitor_tx) = Ckb4IbcEventMonitor::new(
            self.rt.clone(),
            self.rpc_client.clone(),
            self.config.clone(),
            self.counterparty_client_type.subscribe(),
        );
        std::thread::spawn(move || monitor.run());
        Ok(monitor_tx)
    }

    fn fetch_packet_cells_and_extract(
        &self,
        channel_id: &ChannelId,
        port_id: &PortId,
        sequence: Option<Sequence>,
    ) -> Result<Vec<(IbcPacket, CellInput)>, Error> {
        let (sequence, search_all, limit) = if let Some(value) = sequence {
            (u64::from(value) as u16, false, 1)
        } else {
            (0, true, u32::MAX)
        };
        let script = Script::new_builder()
            .code_hash(self.get_converter()?.get_packet_code_hash())
            .hash_type(ScriptHashType::Type.into())
            .args(
                PacketArgs {
                    channel_id: get_channel_number(channel_id)?,
                    port_id: port_id.as_str().as_bytes().try_into().unwrap(),
                    sequence,
                }
                .get_search_args(search_all)
                .pack(),
            )
            .build();
        let search_key = get_search_key(script);
        let packets = self
            .rpc_client
            .fetch_live_cells(search_key, limit, None)
            .and_then(|resp| async move {
                let mut packets = vec![];
                for cell in resp.objects {
                    let tx_hash = &cell.out_point.tx_hash;
                    let tx = self
                        .rpc_client
                        .get_transaction(tx_hash)
                        .await
                        .map_err(|e| Error::query(e.to_string()))?
                        .unwrap()
                        .transaction
                        .unwrap();
                    let tx = match tx.inner {
                        ckb_jsonrpc_types::Either::Left(tx) => tx,
                        ckb_jsonrpc_types::Either::Right(bytes) => {
                            serde_json::from_slice(bytes.as_bytes()).unwrap()
                        }
                    };
                    let ibc_packet = extract_ibc_packet_from_tx(tx)?;
                    let cell_input = CellInput::new_builder()
                        .previous_output(cell.out_point.into())
                        .build();
                    packets.push((ibc_packet, cell_input));
                }
                Ok(packets)
            });
        self.rt.block_on(packets)
    }

    fn fetch_channel_cell_and_extract(
        &self,
        channel_id: &ChannelId,
        port_id: &PortId,
        is_open: bool,
    ) -> Result<(ChannelEnd, IbcChannel), Error> {
        let channel_code_hash = self.get_converter()?.get_channel_code_hash();
        let client_id = self
            .config
            .lc_client_type_hash(self.counterparty_client_type())?;
        let channel_args = ChannelArgs {
            client_id: client_id.into(),
            open: is_open,
            channel_id: get_channel_number(channel_id)?,
            port_id: convert_port_id_to_array(port_id)?,
        };
        let script = Script::new_builder()
            .code_hash(channel_code_hash)
            .args(channel_args.to_args().pack())
            .hash_type(ScriptHashType::Type.into())
            .build();
        let search_key = get_search_key(script);
        let channel_future = self
            .rpc_client
            .fetch_live_cells(search_key, 1, None)
            .and_then(|resp| async move {
                let cell = resp
                    .objects
                    .first()
                    .ok_or(Error::query("no channel cell is fetched".to_string()))?;
                let tx_hash = &cell.out_point.tx_hash;
                let tx_resp = self
                    .rpc_client
                    .get_transaction(tx_hash)
                    .await
                    .map_err(|_| Error::query("fetch back tx failed1".to_string()))?
                    .ok_or(Error::query("fetch back tx failed2".to_string()))?
                    .transaction
                    .unwrap();
                let tx = match tx_resp.inner {
                    ckb_jsonrpc_types::Either::Left(channel) => channel,
                    ckb_jsonrpc_types::Either::Right(json_bytes) => {
                        serde_json::from_slice::<TransactionView>(json_bytes.as_bytes()).unwrap()
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
        let ((channel, ibc_channel_end), cell_input) = self.rt.block_on(channel_future)?;

        let mut data = self.channel_input_data.borrow_mut();
        data.insert((channel.channel_id.clone(), channel.port_id), cell_input);
        let mut cache = self.channel_cache.borrow_mut();
        cache.insert(channel.channel_id, ibc_channel_end.clone());
        Ok((channel.channel_end, ibc_channel_end))
    }

    fn clear_cache(&mut self) {
        self.channel_input_data.get_mut().clear();
        self.channel_cache.get_mut().clear();
        self.packet_input_data.get_mut().clear();
        self.connection_cache.get_mut().clear();
    }

    fn query_connection_and_cache(&self) -> Result<(), Error> {
        let search_key = get_connection_search_key(&self.config, None)?;
        let future = self
            .rpc_client
            // FIXME: use `u32::MAX` to search all cells in one action may be a little heavy if cells are too much,
            //        use `loop` expression to progressivly search
            .fetch_live_cells(search_key, u32::MAX, None)
            .and_then(|response| async {
                let mut resps = vec![];
                for cell in response.objects {
                    let tx = self
                        .rpc_client
                        .get_transaction(&cell.out_point.tx_hash)
                        .await?;
                    let cell_input = CellInput::new_builder()
                        .previous_output(cell.out_point.into())
                        .build();
                    let client_id = hex::encode(cell.output.lock.args.into_bytes());
                    resps.push((tx, cell_input, client_id));
                }
                Ok(resps)
            });
        let mut cache = self.connection_cache.borrow_mut();
        let prefix = self.query_commitment_prefix()?;
        for (transaction, cell_input, client_id) in self.rt.block_on(future)? {
            let tx = transaction
                .expect("empty transaction response")
                .transaction
                .expect("empty transaction view");
            let tx = match tx.inner {
                ckb_jsonrpc_types::Either::Left(tx) => tx,
                ckb_jsonrpc_types::Either::Right(bytes) => {
                    serde_json::from_slice::<TransactionView>(bytes.as_bytes()).unwrap()
                }
            };
            let (connections, ibc_connection) = extract_connections_from_tx(tx, &prefix)?;
            let client_type = self.config.lc_client_type(&client_id)?;
            cache.insert(client_type, (ibc_connection, cell_input, connections));
        }
        Ok(())
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
        let result = result
            .as_advanced_builder()
            // placeholder for the secp256k1 script, it will be used in the signing step
            .witness(WitnessArgs::new_builder().build().as_bytes().pack())
            .witness(witness)
            .build();
        Ok(result)
    }

    fn counterparty_client_type(&self) -> ClientType {
        self.counterparty_client_type
            .borrow()
            .unwrap_or(ClientType::Mock)
    }

    fn sync_counterparty_client_type(&self, client_type: ClientType) {
        self.counterparty_client_type.send_if_modified(|prev| {
            if *prev != Some(client_type) {
                *prev = Some(client_type);
                true
            } else {
                false
            }
        });
    }

    fn fetch_packet_cell_and_extract(
        &self,
        channel_id: &ChannelId,
        port_id: &PortId,
        sequence: Sequence,
    ) -> Result<(IbcPacket, CellInput), Error> {
        let packets = self.fetch_packet_cells_and_extract(channel_id, port_id, Some(sequence))?;
        let (ibc_packet, cell_input) = packets.into_iter().next().ok_or(Error::query(format!(
            "packet not found on ({channel_id}/{port_id}/{sequence})"
        )))?;
        Ok((ibc_packet, cell_input))
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

        #[cfg(not(test))]
        {
            use super::ckb::sighash::init_sighash_celldep;
            rt.block_on(init_sighash_celldep(rpc_client.as_ref()))?;
        }

        let mut client_outpoints = HashMap::new();
        for (
            client_type,
            LightClientItem {
                chain_id,
                client_cell_type_args,
            },
        ) in &config.onchain_light_clients
        {
            let client_cell = rt.block_on(rpc_client.search_cell_by_typescript(
                &TYPE_ID_CODE_HASH.pack(),
                &client_cell_type_args.as_bytes().to_owned(),
            ))?;
            let Some(cell) = client_cell else {
                return Err(Error::other_error(format!(
                    "client cell not found on {chain_id}"
                )));
            };
            client_outpoints.insert(*client_type, cell.out_point);
        }

        let conn_contract_cell = rt.block_on(rpc_client.search_cell_by_typescript(
            &TYPE_ID_CODE_HASH.pack(),
            &config.connection_type_args.as_bytes().to_owned(),
        ))?;
        if conn_contract_cell.is_none() {
            return Err(Error::other_error("connection cell not found".to_owned()));
        }

        let chan_contract_cell = rt.block_on(rpc_client.search_cell_by_typescript(
            &TYPE_ID_CODE_HASH.pack(),
            &config.channel_type_args.as_bytes().to_owned(),
        ))?;
        if chan_contract_cell.is_none() {
            return Err(Error::other_error("channel contract not found".to_owned()));
        }

        let packet_contract_cell = rt.block_on(rpc_client.search_cell_by_typescript(
            &TYPE_ID_CODE_HASH.pack(),
            &config.packet_type_args.as_bytes().to_owned(),
        ))?;
        if packet_contract_cell.is_none() {
            return Err(Error::other_error("packet contract not found".to_owned()));
        }
        let keybase =
            KeyRing::new(Default::default(), "ckb", &config.id).map_err(Error::key_base)?;
        let chain = Ckb4IbcChain {
            rt,
            rpc_client,
            config,
            keybase,
            cached_network: RwLock::new(None),
            tx_monitor_cmd: None,
            client_outpoints: RefCell::new(client_outpoints),
            connection_outpoint: conn_contract_cell.unwrap().out_point,
            channel_outpoint: chan_contract_cell.unwrap().out_point,
            packet_outpoint: packet_contract_cell.unwrap().out_point,
            counterparty_client_type: tokio::sync::watch::channel(None).0,
            channel_input_data: RefCell::new(HashMap::new()),
            channel_cache: RefCell::new(HashMap::new()),
            connection_cache: RefCell::new(HashMap::new()),
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
        let mut tx_hashes = Vec::new();
        let mut events = Vec::new();
        let mut result_events = Vec::new();
        for msg in tracked_msgs.msgs {
            let converter = self.get_converter()?;
            let CkbTxInfo {
                unsigned_tx,
                envelope,
                input_capacity,
                event,
            } = convert_msg_to_ckb_tx(msg, &converter)?;
            if unsigned_tx.is_none() {
                if let Some(e) = event {
                    if let IbcEvent::CreateClient(e) = &e {
                        let client_type = e.0.client_type;
                        info!("the counterparty client type of Ckb4Ibc is set as {client_type}");
                        self.sync_counterparty_client_type(client_type);
                    }
                    let ibc_event = IbcEventWithHeight::new(e, Height::default());
                    result_events.push(ibc_event);
                }
                continue;
            }
            let unsigned_tx = unsigned_tx.unwrap();
            match self.complete_tx_with_secp256k1_change_and_envelope(
                unsigned_tx,
                input_capacity,
                envelope,
            ) {
                Ok(tx) => {
                    let last_input_idx = tx.inputs().len() - 1;
                    let secret_key = self
                        .keybase
                        .get_key(&self.config.key_name)
                        .map_err(Error::key_base)?
                        .into_ckb_keypair(self.network()?)
                        .private_key;
                    let signer = SecpSighashScriptSigner::new(Box::new(
                        SecpCkbRawKeySigner::new_with_secret_keys(vec![secret_key]),
                    ));
                    let tx = signer
                        .sign_tx(
                            &tx,
                            &ScriptGroup {
                                script: Script::from(&self.tx_assembler_address()?),
                                group_type: ScriptGroupType::Lock,
                                // TODO: here should be more indices in case of more than one Secp256k1 cells
                                //       have been filled in the transaction
                                input_indices: vec![last_input_idx],
                                output_indices: vec![],
                            },
                        )
                        .unwrap();
                    tx_hashes.push(tx.hash().unpack());
                    txs.push(tx);
                    events.push(event);
                }
                Err(err) => {
                    // return signing error such as no enough ckb
                    return Err(err);
                }
            }
        }
        let resps = txs.into_iter().map(|tx| {
            let tx: TransactionView = tx.into();
            self.rpc_client
                .send_transaction(&tx.inner, None)
                .and_then(|tx_hash| {
                    wait_ckb_transaction_committed(
                        &self.rpc_client,
                        tx_hash,
                        Duration::from_secs(10),
                        4,
                        Duration::from_secs(600),
                    )
                })
        });
        let responses = self.rt.block_on(futures::future::join_all(resps));
        for (i, response) in responses.iter().enumerate() {
            match response {
                Ok(height) => {
                    if let Some(event) = events.get(i).unwrap().clone() {
                        let tx_hash: [u8; 32] = tx_hashes.get(i).unwrap().clone().into();
                        let ibc_event_with_height = IbcEventWithHeight {
                            event,
                            height: Height::from_noncosmos_height(*height),
                            tx_hash,
                        };
                        result_events.push(ibc_event_with_height);
                    }
                }
                Err(e) => {
                    return Err(Error::send_tx(e.to_string()));
                }
            }
        }
        self.clear_cache();
        Ok(result_events)
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
        _trusted: Height,
        _target: Height,
        _client_state: &AnyClientState,
    ) -> Result<Self::LightBlock, Error> {
        // Ckb4Ibc doesn't have light client module until we insert cell-emitter crate to
        // relay ckb headers and cells to Axon endpoint
        Ok(CkbLightBlock {})
    }

    fn check_misbehaviour(
        &mut self,
        _update: &UpdateClient,
        _client_state: &AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        // Ckb4Ibc doesn't have to check the misbehaviour on Axon's metadata cell
        Ok(None)
    }

    fn query_balance(&self, address: Option<&str>, symbol: Option<&str>) -> Result<Balance, Error> {
        let lock_script: Script = match address {
            Some(address) => Address::from_str(address)
                .map_err(|e| {
                    Error::invalid_key_address(
                        address.to_string(),
                        tendermint::Error::invalid_key(e.to_string()),
                    )
                })?
                .payload()
                .into(),
            None => self.tx_assembler_address()?.payload().into(),
        };
        let search_key = match symbol {
            Some(symbol) => get_search_key_with_sudt(lock_script, symbol, self.network()?)?,
            None => get_search_key(lock_script),
        };
        let asset_cells =
            self.rt
                .block_on(self.rpc_client.fetch_live_cells(search_key, u32::MAX, None))?;
        let balance: u128 = asset_cells
            .objects
            .into_iter()
            .filter_map(|cell| {
                if symbol.is_some() {
                    let data: [u8; 16] = cell
                        .output_data
                        .unwrap()
                        .as_bytes()
                        .try_into()
                        .expect("sudt capacity");
                    Some(u128::from_le_bytes(data))
                } else if cell.output.type_.is_some() {
                    None
                } else {
                    Some(cell.output.capacity.value() as u128)
                }
            })
            .sum();
        Ok(Balance {
            amount: balance.to_string(),
            denom: symbol.unwrap_or("ckb").to_owned(),
        })
    }

    fn query_all_balances(&self, address: Option<&str>) -> Result<Vec<Balance>, Error> {
        let ckb_balance = self.query_balance(address, None)?;
        Ok(vec![ckb_balance])
    }

    fn query_denom_trace(&self, _hash: String) -> Result<DenomTrace, Error> {
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
        let header = self.rt.block_on(self.rpc_client.get_tip_header())?;
        let height = Height::from_noncosmos_height(header.inner.number.value());
        let ts_milisec = header.inner.timestamp.value();
        let timestamp = Timestamp::from_nanoseconds(ts_milisec * 1_000_000).unwrap();
        Ok(ChainStatus { height, timestamp })
    }

    fn query_clients(
        &self,
        _request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        Ok(self
            .config
            .onchain_light_clients
            .keys()
            .map(|client_type| {
                let client_id = self.config.lc_client_id(*client_type).unwrap();
                IdentifiedAnyClientState {
                    client_id,
                    client_state: CkbClientState {
                        chain_id: self.id(),
                        latest_height: Height::default(),
                    }
                    .into(),
                }
            })
            .collect())
    }

    fn query_client_state(
        &self,
        request: QueryClientStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyClientState, Option<MerkleProof>), Error> {
        let chain_id = self.config.lc_chain_id(&request.client_id.to_string())?;
        let client_state = CkbClientState {
            chain_id,
            latest_height: Height::default(),
        };
        let client_type = self.config.lc_client_type(&request.client_id.to_string())?;
        self.sync_counterparty_client_type(client_type);
        Ok((client_state.into(), None))
    }

    fn query_consensus_state(
        &self,
        _request: QueryConsensusStateRequest,
        _include_proof: IncludeProof,
    ) -> Result<(AnyConsensusState, Option<MerkleProof>), Error> {
        // TODO: fix it when Ckb4Ibc contract refactorred
        Ok((CkbConsensusState {}.into(), None))
    }

    fn query_consensus_state_heights(
        &self,
        _request: QueryConsensusStateHeightsRequest,
    ) -> Result<Vec<Height>, Error> {
        // TODO: fix it when Ckb4Ibc contract refactorred
        Ok(vec![Height::default()])
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
        self.query_connection_and_cache()?;
        let connections = self
            .connection_cache
            .borrow()
            .iter()
            .flat_map(|(_, (_, _, connection))| connection.clone())
            .collect();
        Ok(connections)
    }

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        self.query_connection_and_cache()?;
        let client_type = self.config.lc_client_type(&request.client_id.to_string())?;
        if let Some((_, _, connection)) = self.connection_cache.borrow().get(&client_type) {
            self.sync_counterparty_client_type(client_type);
            let connection_ids = connection.iter().map(|v| v.connection_id.clone()).collect();
            Ok(connection_ids)
        } else {
            Ok(vec![])
        }
    }

    fn query_connection(
        &self,
        request: QueryConnectionRequest,
        _include_proof: IncludeProof,
    ) -> Result<(ConnectionEnd, Option<MerkleProof>), Error> {
        let connections = self.query_connections(QueryConnectionsRequest { pagination: None })?;
        let index = get_connection_index_by_id(&request.connection_id)? as usize;
        let connection = connections
            .into_iter()
            .nth(index)
            .ok_or(Error::ckb_conn_id_invalid(
                request.connection_id.as_str().to_string(),
            ))?
            .connection_end;
        let client_type = self
            .config
            .lc_client_type(&connection.client_id().to_string())?;
        self.sync_counterparty_client_type(client_type);
        Ok((connection, None))
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
        let channel_code_hash = self.get_converter()?.get_channel_code_hash();
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
                (u32::MAX, 0)
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
            .filter_map(|tx| {
                if let Ok(Some(tx)) = tx {
                    if tx.tx_status.status == Status::Committed {
                        return Some(tx);
                    }
                }
                None
            })
            .flat_map(|tx| {
                let tx = match tx.transaction.unwrap().inner {
                    ckb_jsonrpc_types::Either::Left(tx) => tx,
                    ckb_jsonrpc_types::Either::Right(bytes) => {
                        serde_json::from_slice::<TransactionView>(bytes.as_bytes()).unwrap()
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
        if let Ok((channel, _)) =
            self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, false)
        {
            Ok((channel, None))
        } else {
            let (channel, _) =
                self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
            Ok((channel, None))
        }
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        let request = QueryChannelRequest {
            port_id: request.port_id,
            channel_id: request.channel_id,
            height: QueryHeight::Latest,
        };
        let (channel, _) = self.query_channel(request, IncludeProof::No)?;
        assert!(!channel.connection_hops.is_empty());
        let request = QueryConnectionRequest {
            connection_id: channel.connection_hops[0].clone(),
            height: QueryHeight::Latest,
        };
        let (connection, _) = self.query_connection(request, IncludeProof::No)?;
        let request = QueryClientStateRequest {
            client_id: connection.client_id().clone(),
            height: QueryHeight::Latest,
        };
        let (client, _) = self.query_client_state(request, IncludeProof::No)?;
        Ok(Some(IdentifiedAnyClientState {
            client_id: connection.client_id().clone(),
            client_state: client,
        }))
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
            let commitment = keccak256(&ibc_packet.rlp_bytes()).to_vec();
            Ok((commitment, None))
        }
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        // get all packets' commitment without pagination
        let sequences = self
            .fetch_packet_cells_and_extract(&request.channel_id, &request.port_id, None)?
            .into_iter()
            .map(|(v, _)| (v.packet.sequence as u64).into())
            .collect();
        Ok((sequences, Height::default()))
    }

    fn query_packet_receipt(
        &self,
        request: QueryPacketReceiptRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let generate_receipt = |channel: IbcChannel| {
            if channel.order == Ordering::Unordered {
                Ok((vec![1u8], None))
            } else {
                Ok((vec![], None))
            }
        };
        let (_, channel) =
            self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
        if channel.sequence.next_sequence_recvs as u64 > request.sequence.into() {
            generate_receipt(channel)
        } else {
            let (ibc_packet, _) = self.fetch_packet_cell_and_extract(
                &request.channel_id,
                &request.port_id,
                request.sequence,
            )?;
            if ibc_packet.status != PacketStatus::Recv {
                Ok((vec![], None))
            } else {
                generate_receipt(channel)
            }
        }
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let (_, channel) =
            self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
        let sequences = request
            .packet_commitment_sequences
            .into_iter()
            .filter(|sequence| {
                if (channel.sequence.next_sequence_recvs as u64) < (*sequence).into() {
                    return false;
                }
                let Ok((packet, _)) = self.fetch_packet_cell_and_extract(
                    &request.channel_id,
                    &request.port_id,
                    *sequence,
                ) else {
                    return true;
                };
                if packet.status != PacketStatus::Recv {
                    return true;
                }
                false
            })
            .collect();
        Ok(sequences)
    }

    fn query_packet_acknowledgement(
        &self,
        request: QueryPacketAcknowledgementRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Vec<u8>, Option<MerkleProof>), Error> {
        let result = self.fetch_packet_cell_and_extract(
            &request.channel_id,
            &request.port_id,
            request.sequence,
        );
        if let Ok((ibc_packet, _)) = result {
            if ibc_packet.status != PacketStatus::WriteAck || ibc_packet.status != PacketStatus::Ack
            {
                Ok((vec![], None))
            } else {
                // no matter the `data` is empty or not, we recognize the acknowledge is committed
                // if the `WriteAck` cell exists
                let ack_commitment = keccak256(&ibc_packet.packet.data).to_vec();
                Ok((ack_commitment, None))
            }
        } else {
            // check the sequence status in channel if the packet cell under required sequence not found
            let (_, channel) =
                self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
            if channel.sequence.next_sequence_recvs as u64 > request.sequence.into() {
                // since the previous `WriteAck` cells are consumed and hard to fetch from CKB, we
                // just use mock acknowledgement in return which in Hermes runtime is not sensitive
                // with its real content but just used as Yes or No option
                let ack_commitment = keccak256(b"unfetchable acknowledgement").to_vec();
                Ok((ack_commitment, None))
            } else {
                Ok((vec![], None))
            }
        }
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<Sequence>, Height), Error> {
        let sequences = request
            .packet_commitment_sequences
            .into_iter()
            .filter(|sequence| {
                let Ok((acknowledgement, _)) = self.query_packet_acknowledgement(
                    QueryPacketAcknowledgementRequest {
                        port_id: request.port_id.clone(),
                        channel_id: request.channel_id.clone(),
                        sequence: *sequence,
                        height: QueryHeight::Latest,
                    },
                    IncludeProof::No,
                ) else {
                    return false;
                };
                if acknowledgement.is_empty() {
                    return false;
                }
                true
            })
            .collect();
        Ok((sequences, Height::default()))
    }

    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<Sequence>, Error> {
        let (_, channel) =
            self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
        let sequences = request
            .packet_ack_sequences
            .into_iter()
            .filter(|sequence| {
                if channel.sequence.next_sequence_acks as u64 > (*sequence).into() {
                    return false;
                }
                let Ok((packet, _)) = self.fetch_packet_cell_and_extract(
                        &request.channel_id,
                        &request.port_id,
                        *sequence,
                ) else {
                    return true;
                };
                if packet.status != PacketStatus::Ack {
                    return true;
                }
                false
            })
            .collect();
        Ok(sequences)
    }

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        _include_proof: IncludeProof,
    ) -> Result<(Sequence, Option<MerkleProof>), Error> {
        let (_, ibc_channel) =
            self.fetch_channel_cell_and_extract(&request.channel_id, &request.port_id, true)?;
        let sequence = (ibc_channel.sequence.next_sequence_recvs as u64).into();
        Ok((sequence, None))
    }

    fn query_txs(&self, _request: QueryTxRequest) -> Result<Vec<IbcEventWithHeight>, Error> {
        warn!("ckb4ibc query_txs() not support");
        Ok(vec![])
    }

    fn query_packet_events(
        &self,
        _request: QueryPacketEventDataRequest,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        warn!("ckb4ibc query_packet_events() not support");
        Ok(vec![])
    }

    fn query_host_consensus_state(
        &self,
        _request: QueryHostConsensusStateRequest,
    ) -> Result<Self::ConsensusState, Error> {
        // TODO
        warn!("axon query_host_consensus_state() not support");
        Ok(CkbConsensusState {})
    }

    fn build_client_state(
        &self,
        height: Height,
        settings: ClientSettings,
    ) -> Result<Self::ClientState, Error> {
        match settings {
            ClientSettings::AxonCkb | ClientSettings::Other => Ok(CkbClientState {
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
        Ok(CkbConsensusState {})
    }

    fn build_header(
        &mut self,
        _trusted_height: Height,
        _target_height: Height,
        _client_state: &AnyClientState,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error> {
        Ok((CkbHeader {}, vec![]))
    }

    fn maybe_register_counterparty_payee(
        &mut self,
        _channel_id: &ChannelId,
        _port_id: &PortId,
        _counterparty_payee: &Signer,
    ) -> Result<(), Error> {
        warn!("ckb4ibc maybe_register_counterparty_payee() not support");
        Ok(())
    }

    fn cross_chain_query(
        &self,
        _requests: Vec<CrossChainQueryRequest>,
    ) -> Result<Vec<CrossChainQueryResponse>, Error> {
        warn!("ckb4ibc cross_chain_query() not support");
        Ok(vec![])
    }

    fn query_incentivized_packet(
        &self,
        _request: QueryIncentivizedPacketRequest,
    ) -> Result<QueryIncentivizedPacketResponse, Error> {
        warn!("ckb4ibc query_incentivized_packet() not support");
        Ok(QueryIncentivizedPacketResponse {
            incentivized_packet: None,
        })
    }

    fn id(&self) -> ChainId {
        self.config().id().clone()
    }

    fn build_connection_proofs_and_client_state(
        &self,
        _message_type: ConnectionMsgType,
        _connection_id: &ConnectionId,
        _client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error> {
        Ok((
            Some(AnyClientState::Ckb(CkbClientState {
                chain_id: self.id(),
                latest_height: height,
            })),
            // FIXME: fix it until the hardfork of CKB
            get_dummy_merkle_proof(height),
        ))
    }

    fn build_channel_proofs(
        &self,
        _port_id: &PortId,
        _channel_id: &ChannelId,
        height: Height,
    ) -> Result<Proofs, Error> {
        // FIXME: fix it until the hardfork of CKB
        Ok(get_dummy_merkle_proof(height))
    }

    fn build_packet_proofs(
        &self,
        _packet_type: PacketMsgType,
        _port_id: PortId,
        _channel_id: ChannelId,
        _sequence: Sequence,
        height: Height,
    ) -> Result<Proofs, Error> {
        // FIXME: fix it until the hardfork of CKB
        Ok(get_dummy_merkle_proof(height))
    }
}
