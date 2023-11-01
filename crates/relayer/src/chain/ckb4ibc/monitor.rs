use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use ckb_ics_axon::handler::{IbcPacket, PacketStatus};
use ckb_ics_axon::object::State as CkbState;
use ckb_ics_axon::ChannelArgs;
use ckb_jsonrpc_types::{JsonBytes, Status, TransactionView};
use ckb_sdk::rpc::ckb_indexer::SearchKey;
use ckb_types::core::ScriptHashType;
use ckb_types::packed::{CellInput, Script};
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::H256;
use crossbeam_channel::{Receiver, Sender};
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::height::Height;
use ibc_relayer_types::core::ics03_connection::events::{
    Attributes, OpenInit as ConnectionOpenInit, OpenTry as ConnectionOpenTry,
};
use ibc_relayer_types::core::ics04_channel::channel::State;
use ibc_relayer_types::core::ics04_channel::events::{
    OpenInit as ChannelOpenInit, OpenTry as ChannelOpenTry, SendPacket, WriteAcknowledgement,
};
use ibc_relayer_types::core::ics04_channel::packet::{Packet, Sequence};
use ibc_relayer_types::core::ics04_channel::timeout::TimeoutHeight;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ClientId, PortId};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::timestamp::Timestamp;
use tokio::runtime::Runtime as TokioRuntime;
use tokio::sync::watch::Receiver as WatchReceiver;
use tracing::{error, info};

use crate::chain::ckb::prelude::CkbReader;
use crate::chain::ckb::rpc_client::RpcClient;
use crate::chain::ckb4ibc::extractor::{
    extract_channel_end_from_tx, extract_ibc_connections_from_tx, extract_ibc_packet_from_tx,
};
use crate::chain::tracking::TrackingId;
use crate::chain::SEC_TO_NANO;
use crate::config::ckb4ibc::ChainConfig;
use crate::event::bus::EventBus;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use crate::event::IbcEventWithHeight;

use super::cache_set::CacheSet;
use super::utils::{
    generate_connection_id, get_prefix_search_key, get_script_hash, tip_block_number,
};

#[derive(Eq, PartialOrd, Ord, PartialEq, Hash, Clone, Copy)]
pub enum IbcProtocolType {
    Connection,
    Channel,
    Packet,
}

pub type WriteAckMonitorSender = (Sender<Option<(IbcPacket, CellInput)>>, u64);
pub type WriteAckMonitorCmd = Sender<WriteAckMonitorSender>;

// TODO: add cell emitter here
pub struct Ckb4IbcEventMonitor {
    rt: Arc<TokioRuntime>,
    rpc_client: Arc<RpcClient>,
    rx_cmd: Receiver<MonitorCmd>,
    rx_write_ack: Receiver<WriteAckMonitorSender>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
    config: ChainConfig,
    cache_set: RwLock<CacheSet<H256>>,
    counterparty_client_type_rx: WatchReceiver<Option<ClientType>>,
    counterparty_client_type: ClientType,
    fetch_cursors: HashMap<IbcProtocolType, JsonBytes>,
    useless_write_ack_packets: BTreeMap<u64, (IbcPacket, CellInput)>,
}

impl Ckb4IbcEventMonitor {
    pub fn new(
        rt: Arc<TokioRuntime>,
        rpc_client: Arc<RpcClient>,
        config: ChainConfig,
        counterparty_client_type_rx: WatchReceiver<Option<ClientType>>,
    ) -> (Self, TxMonitorCmd, WriteAckMonitorCmd) {
        let (tx_cmd, rx_cmd) = crossbeam_channel::unbounded();
        let (tx_write_ack, rx_write_ack) = crossbeam_channel::unbounded();
        let monitor = Ckb4IbcEventMonitor {
            rt,
            rpc_client,
            rx_cmd,
            rx_write_ack,
            event_bus: EventBus::default(),
            config,
            cache_set: RwLock::new(CacheSet::new(512)),
            counterparty_client_type_rx,
            counterparty_client_type: ClientType::Mock,
            fetch_cursors: HashMap::new(),
            useless_write_ack_packets: BTreeMap::new(),
        };
        (monitor, TxMonitorCmd::new(tx_cmd), tx_write_ack)
    }

    pub fn run(mut self) {
        let rt = self.rt.clone();
        // Block here until the counterparty is revealed.
        rt.block_on(async {
            self.counterparty_client_type = self
                .counterparty_client_type_rx
                .wait_for(|t| t.is_some())
                .await
                .expect("counterparty_client_type sender is closed")
                // Unwrapping is OK because the value is Some.
                .unwrap();
        });
        info!(
            "{} async counterparty_client_type received: {}, starting IBC events listen process",
            self.config.id, self.counterparty_client_type
        );
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let result = rt.block_on(self.run_once());
            match result {
                Next::Continue => continue,
                Next::Abort => break,
            }
        }
    }

    async fn handle_get_useless_write_ack_packet(&mut self) -> Result<()> {
        if let Ok((resposne, block_number_gap)) = self.rx_write_ack.try_recv() {
            let useless_key = self.useless_write_ack_packets.keys().next().cloned();
            if let Some(block_number) = useless_key {
                let tip_block_number = tip_block_number(self.rpc_client.as_ref())
                    .await
                    .map_err(|err| Error::others(err.detail().to_string()))?;
                if block_number + block_number_gap < tip_block_number {
                    let (packet, input) = self
                        .useless_write_ack_packets
                        .remove(&block_number)
                        .unwrap();
                    resposne.send(Some((packet, input))).unwrap();
                    return Ok(());
                }
            }
            resposne.send(None).unwrap();
        }
        Ok(())
    }

    async fn run_once(&mut self) -> Next {
        if let Ok(cmd) = self.rx_cmd.try_recv() {
            match cmd {
                MonitorCmd::Shutdown => return Next::Abort,
                MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
            }
        }

        // 'mut self' cannot be used in tokio::join macro, it can only be handled in sequence
        let connection_events = self.fetch_connection_events().await;
        let channel_events = self.fetch_channel_events().await;
        let packet_events = self.fetch_packet_events().await;

        self.process_batch(connection_events);
        self.process_batch(channel_events);
        self.process_batch(packet_events);

        if let Err(err) = self.handle_get_useless_write_ack_packet().await {
            error!("{err}");
        }

        Next::Continue
    }

    async fn fetch_connection_events(&mut self) -> Result<EventBatch> {
        let connection_code_hash = get_script_hash(&self.config.connection_type_args);
        let client_type_hash = self
            .config
            .lc_client_type_hash(self.counterparty_client_type)
            .map_err(|e| Error::collect_events_failed(e.to_string()))?;
        let client_id: ClientId = hex::encode(client_type_hash.0).parse().unwrap();
        let script = Script::new_builder()
            .code_hash(connection_code_hash)
            .hash_type(ScriptHashType::Type.into())
            .args(client_type_hash.as_bytes().pack())
            .build();
        let key = get_prefix_search_key(script);
        let connections = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj = extract_ibc_connections_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("channel".to_string()))?;
                    Ok((obj, hash))
                },
                1,
                IbcProtocolType::Connection,
            )
            .await?;
        if connections.is_empty() {
            return Ok(EventBatch {
                chain_id: self.config.id.clone(),
                tracking_id: TrackingId::Static("ckb connection events collection"),
                height: Height::default(),
                events: vec![],
            });
        }
        let ((ibc_connection_cell, tx_hash), (block_number, _)) =
            connections.into_iter().next().unwrap();
        if self.cache_set.read().unwrap().has(&tx_hash) {
            return Ok(EventBatch {
                chain_id: self.config.id.clone(),
                tracking_id: TrackingId::Static("ckb connection events collection"),
                height: Height::from_noncosmos_height(block_number),
                events: vec![],
            });
        }
        self.cache_set.write().unwrap().insert(tx_hash.clone());
        let events = ibc_connection_cell
            .connections
            .into_iter()
            .enumerate()
            .flat_map(|(idx, connection_end)| match connection_end.state {
                CkbState::Init => {
                    let connection_id = generate_connection_id(idx as u16, client_id.as_str());
                    info!(
                        "🫡  {} received ConnectionOpenInit event, connection_id = {connection_id}",
                        self.config.id
                    );
                    let attrs = Attributes {
                        connection_id: Some(connection_id),
                        client_id: client_id.clone(),
                        counterparty_connection_id: None,
                        counterparty_client_id: ClientId::from_str(
                            &connection_end.counterparty.client_id,
                        )
                        .unwrap(),
                    };
                    let event = IbcEvent::OpenInitConnection(ConnectionOpenInit(attrs));
                    Some(IbcEventWithHeight {
                        event,
                        height: Height::from_noncosmos_height(block_number),
                        tx_hash: tx_hash.clone().into(),
                    })
                }
                CkbState::OpenTry => {
                    let connection_id = generate_connection_id(idx as u16, client_id.as_str());
                    info!(
                        "🫡  {} received ConnectionOpenTry event, connection_id = {connection_id}",
                        self.config.id
                    );
                    let attrs = Attributes {
                        connection_id: Some(connection_id),
                        client_id: client_id.clone(),
                        counterparty_connection_id: None,
                        counterparty_client_id: ClientId::from_str(
                            &connection_end.counterparty.client_id,
                        )
                        .unwrap(),
                    };
                    let event = IbcEvent::OpenTryConnection(ConnectionOpenTry(attrs));
                    Some(IbcEventWithHeight {
                        event,
                        height: Height::from_noncosmos_height(block_number),
                        tx_hash: tx_hash.clone().into(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        let tip_block_number = tip_block_number(self.rpc_client.as_ref())
            .await
            .map_err(|err| Error::others(err.detail().to_string()))?;
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb connection events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn fetch_channel_events(&mut self) -> Result<EventBatch> {
        let client_id = self
            .config
            .lc_client_type_hash(self.counterparty_client_type)
            .map_err(|e| Error::collect_events_failed(e.to_string()))?;
        let channel_args = ChannelArgs {
            client_id: client_id.into(),
            open: false,
            ..Default::default()
        };
        let script = Script::new_builder()
            .code_hash(get_script_hash(&self.config.channel_type_args))
            .hash_type(ScriptHashType::Type.into())
            .args(channel_args.get_prefix_for_searching_unopen().pack())
            .build();

        let key = get_prefix_search_key(script);
        let identified_channel_ends = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj = extract_channel_end_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("channel".to_string()))?
                        .0;
                    Ok((obj, hash))
                },
                5,
                IbcProtocolType::Channel,
            )
            .await?;

        let events = identified_channel_ends
            .into_iter()
            .filter(|((_, tx), _)| {
                if self.cache_set.read().unwrap().has(tx) {
                    return false;
                }
                self.cache_set.write().unwrap().insert(tx.clone());
                true
            })
            .map(|((channel, tx), (block_number, _))| match channel.channel_end.state {
                State::Init => {
                    let connection_id = channel.channel_end.connection_hops[0].clone();
                    info!(
                        "🫡  {} received ChannelOpenInit event, channel_id = {}, connection_id = {connection_id}", 
                        self.config.id,
                        channel.channel_id
                    );
                    IbcEventWithHeight {
                        event: IbcEvent::OpenInitChannel(ChannelOpenInit {
                            port_id: channel.port_id,
                            channel_id: Some(channel.channel_id),
                            connection_id,
                            counterparty_port_id: channel.channel_end.remote.port_id,
                            counterparty_channel_id: channel.channel_end.remote.channel_id,
                        }),
                        height: Height::from_noncosmos_height(block_number),
                        tx_hash: tx.into(),
                    }
                },
                State::TryOpen => {
                    let connection_id = channel.channel_end.connection_hops[0].clone();
                    info!(
                        "🫡  {} received ChannelOpenTry event, channel_id = {}, connection_id = {connection_id}", 
                        self.config.id,
                        channel.channel_id
                    );
                    IbcEventWithHeight {
                        event: IbcEvent::OpenTryChannel(ChannelOpenTry {
                            port_id: channel.port_id,
                            channel_id: Some(channel.channel_id),
                            connection_id,
                            counterparty_port_id: channel.channel_end.remote.port_id,
                            counterparty_channel_id: channel.channel_end.remote.channel_id,
                        }),
                        height: Height::from_noncosmos_height(block_number),
                        tx_hash: tx.into(),
                    }
                },
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        let tip_block_number = tip_block_number(self.rpc_client.as_ref())
            .await
            .map_err(|err| Error::others(err.detail().to_string()))?;
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb channel events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn fetch_packet_events(&mut self) -> Result<EventBatch> {
        let script = Script::new_builder()
            .code_hash(get_script_hash(&self.config.packet_type_args))
            .hash_type(ScriptHashType::Type.into())
            .build();
        let key = get_prefix_search_key(script);
        let ibc_packets = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj_with_content = extract_ibc_packet_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("packet".to_string()))?;
                    Ok((obj_with_content, hash))
                },
                10,
                IbcProtocolType::Packet,
            )
            .await?;

        let tip_block_number = tip_block_number(self.rpc_client.as_ref())
            .await
            .map_err(|err| Error::others(err.detail().to_string()))?;

        let useless_packets = &mut self.useless_write_ack_packets;
        let events = ibc_packets
            .into_iter()
            .filter(|(((packet, _), tx), _)| {
                if packet.status == PacketStatus::Ack
                    || packet.status == PacketStatus::Recv
                    || self.cache_set.read().unwrap().has(tx)
                {
                    return false;
                }
                self.cache_set.write().unwrap().insert(tx.clone());
                true
            })
            .map(
                |(((packet, _content), tx), (block_number, cell_input))| match packet.status {
                    PacketStatus::Send => {
                        info!(
                            "🫡  {} received SendPacket({}) event, from {}/{} to {}/{}",
                            self.config.id,
                            packet.packet.sequence,
                            packet.packet.source_channel_id,
                            packet.packet.source_port_id,
                            packet.packet.destination_channel_id,
                            packet.packet.destination_port_id,
                        );
                        IbcEventWithHeight {
                            event: IbcEvent::SendPacket(SendPacket {
                                packet: convert_packet(packet),
                            }),
                            height: Height::from_noncosmos_height(block_number),
                            tx_hash: tx.into(),
                        }
                    }
                    PacketStatus::WriteAck => {
                        info!(
                            "🫡  {} received WriteAck({}) event, from {}/{} to {}/{}",
                            self.config.id,
                            packet.packet.sequence,
                            packet.packet.source_channel_id,
                            packet.packet.source_port_id,
                            packet.packet.destination_channel_id,
                            packet.packet.destination_port_id,
                        );
                        useless_packets.insert(block_number, (packet.clone(), cell_input));
                        IbcEventWithHeight {
                            event: IbcEvent::WriteAcknowledgement(WriteAcknowledgement {
                                ack: packet
                                    .ack
                                    .clone()
                                    .expect("write ack packet should have ack"),
                                packet: convert_packet(packet),
                            }),
                            height: Height::from_noncosmos_height(block_number),
                            tx_hash: tx.into(),
                        }
                    }
                    PacketStatus::Ack | PacketStatus::Recv => unreachable!(),
                },
            )
            .collect::<Vec<_>>();

        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb packet events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn search_and_extract<T, F>(
        &mut self,
        search_key: SearchKey,
        extractor: &F,
        limit: u32,
        ibc_protocol: IbcProtocolType,
    ) -> Result<Vec<((T, H256), (u64, CellInput))>>
    where
        F: Fn(TransactionView) -> Result<(T, H256)>,
    {
        let cursor = self.fetch_cursors.get(&ibc_protocol).cloned();
        let cells = self
            .rpc_client
            .fetch_live_cells(search_key, limit, cursor)
            .await
            .map_err(|_| Error::collect_events_failed("fetch ibc cells failed".to_string()))?;

        let block_number_and_cell_inputs = cells
            .objects
            .iter()
            .map(|cell| {
                let cell_input = CellInput::new_builder()
                    .previous_output(cell.out_point.clone().into())
                    .build();
                (cell.block_number.into(), cell_input)
            })
            .collect::<Vec<(u64, CellInput)>>();
        let ibc_response = cells
            .objects
            .iter()
            .map(|cell| self.rpc_client.get_transaction(&cell.out_point.tx_hash));

        let ibc_iterator = futures::future::join_all(ibc_response)
            .await
            .into_iter()
            .zip(block_number_and_cell_inputs)
            .filter_map(|(tx, number_input)| {
                if let Ok(Some(tx)) = tx {
                    if tx.tx_status.status == Status::Committed && tx.transaction.is_some() {
                        return Some((tx.transaction.unwrap(), number_input));
                    }
                }
                None
            });

        let mut result = vec![];
        for (tx, number_input) in ibc_iterator {
            let tx = match tx.inner {
                ckb_jsonrpc_types::Either::Left(tx) => tx,
                ckb_jsonrpc_types::Either::Right(json) => {
                    serde_json::from_slice(json.as_bytes()).unwrap()
                }
            };
            result.push((extractor(tx)?, number_input));
        }

        if cells.objects.is_empty() {
            self.fetch_cursors.remove(&ibc_protocol);
        } else {
            self.fetch_cursors.insert(ibc_protocol, cells.last_cursor);
        }
        Ok(result)
    }

    fn process_batch(&mut self, batch: Result<EventBatch>) {
        match batch {
            Ok(batch) => {
                if !batch.events.is_empty() {
                    self.event_bus.broadcast(Arc::new(Ok(batch)))
                }
            }
            Err(error) => error!("{error}"),
        }
    }
}

fn convert_packet(packet: IbcPacket) -> Packet {
    assert!(!packet.packet.data.is_empty(), "empty packet data");
    let sequence = Sequence::from(packet.packet.sequence as u64);
    let source_port = PortId::from_str(&packet.packet.source_port_id).unwrap();
    let source_channel = ChannelId::from_str(&packet.packet.source_channel_id).unwrap();
    let destination_port = PortId::from_str(&packet.packet.destination_port_id).unwrap();
    let destination_channel = ChannelId::from_str(&packet.packet.destination_channel_id).unwrap();
    let timeout_height = if packet.packet.timeout_height > 0 {
        TimeoutHeight::At(Height::from_noncosmos_height(packet.packet.timeout_height))
    } else {
        TimeoutHeight::Never
    };
    let timeout_timestamp =
        Timestamp::from_nanoseconds(packet.packet.timeout_timestamp * SEC_TO_NANO).unwrap();
    Packet {
        sequence,
        source_port,
        source_channel,
        destination_port,
        destination_channel,
        data: packet.packet.data,
        timeout_height,
        timeout_timestamp,
    }
}
