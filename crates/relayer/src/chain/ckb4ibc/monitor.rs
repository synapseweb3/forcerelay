use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use ckb_ics_axon::handler::{IbcPacket, PacketStatus};
use ckb_ics_axon::object::State as CkbState;
use ckb_ics_axon::ChannelArgs;
use ckb_jsonrpc_types::{Status, TransactionView};
use ckb_sdk::rpc::ckb_indexer::SearchKey;
use ckb_types::core::ScriptHashType;
use ckb_types::packed::Script;
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::H256;
use crossbeam_channel::Receiver;
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
use super::utils::{generate_connection_id, get_script_hash, get_search_key};

// TODO: add cell emitter here
pub struct Ckb4IbcEventMonitor {
    rt: Arc<TokioRuntime>,
    rpc_client: Arc<RpcClient>,
    rx_cmd: Receiver<MonitorCmd>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
    config: ChainConfig,
    cache_set: RwLock<CacheSet<H256>>,
    counterparty_client_type_rx: WatchReceiver<Option<ClientType>>,
    counterparty_client_type: ClientType,
}

impl Ckb4IbcEventMonitor {
    pub fn new(
        rt: Arc<TokioRuntime>,
        rpc_client: Arc<RpcClient>,
        config: ChainConfig,
        counterparty_client_type_rx: WatchReceiver<Option<ClientType>>,
    ) -> (Self, TxMonitorCmd) {
        let (tx_cmd, rx_cmd) = crossbeam_channel::unbounded();
        let monitor = Ckb4IbcEventMonitor {
            rt,
            rpc_client,
            rx_cmd,
            event_bus: EventBus::default(),
            config,
            cache_set: RwLock::new(CacheSet::new(512)),
            counterparty_client_type_rx,
            counterparty_client_type: ClientType::Mock,
        };
        (monitor, TxMonitorCmd::new(tx_cmd))
    }

    pub fn run(mut self) {
        let rt = self.rt.clone();
        // Block here until the counterparty is revealed.
        info!("listening counterparty_client_type from Endpoint");
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
            "counterparty_client_type received: {}",
            self.counterparty_client_type
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

    async fn run_once(&mut self) -> Next {
        if let Ok(cmd) = self.rx_cmd.try_recv() {
            match cmd {
                MonitorCmd::Shutdown => return Next::Abort,
                MonitorCmd::Subscribe(tx) => tx.send(self.event_bus.subscribe()).unwrap(),
            }
        }
        self.process_batch(self.fetch_connection_events().await);
        self.process_batch(self.fetch_channel_events().await);
        self.process_batch(self.fetch_packet_events().await);
        Next::Continue
    }

    async fn fetch_connection_events(&self) -> Result<EventBatch> {
        let connection_code_hash = get_script_hash(&self.config.connection_type_args);
        let client_id = self
            .config
            .lc_client_type_hash(self.counterparty_client_type)
            .map_err(|e| Error::collect_events_failed(e.to_string()))?;
        let script = Script::new_builder()
            .code_hash(connection_code_hash)
            .hash_type(ScriptHashType::Type.into())
            .args(client_id.as_bytes().pack())
            .build();
        let key = get_search_key(script);
        let ((ibc_connection_cell, tx_hash), block_number) = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj = extract_ibc_connections_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("channel".to_string()))?;
                    Ok((obj, hash))
                },
                1,
            )
            .await?
            .into_iter()
            .next()
            .unwrap();
        if self.cache_set.read().unwrap().has(&tx_hash) {
            return Ok(EventBatch {
                chain_id: self.config.id.clone(),
                tracking_id: TrackingId::Static("ckb connection events collection"),
                height: Height::from_noncosmos_height(block_number),
                events: vec![],
            });
        }
        let client_id = self
            .config
            .lc_client_id(ClientType::Ckb4Ibc)
            .expect("ckb4ibc client_id");
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

        let tip_block_number: u64 = self
            .rpc_client
            .get_tip_header()
            .await
            .unwrap()
            .inner
            .number
            .into();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb connection events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn fetch_channel_events(&self) -> Result<EventBatch> {
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

        let key = get_search_key(script);
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
            .map(|((channel, tx), block_number)| match channel.channel_end.state {
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

        let tip_block_number: u64 = self
            .rpc_client
            .get_tip_header()
            .await
            .unwrap()
            .inner
            .number
            .into();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb channel events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn fetch_packet_events(&self) -> Result<EventBatch> {
        let script = Script::new_builder()
            .code_hash(get_script_hash(&self.config.packet_type_args))
            .hash_type(ScriptHashType::Type.into())
            .args("".pack())
            .build();
        let key = get_search_key(script);
        let ibc_packets = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj_with_content = extract_ibc_packet_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("packet".to_string()))?;
                    Ok((obj_with_content, hash))
                },
                5,
            )
            .await?;

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
                |(((packet, content), tx), block_number)| match packet.status {
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
                        IbcEventWithHeight {
                            event: IbcEvent::WriteAcknowledgement(WriteAcknowledgement {
                                packet: convert_packet(packet),
                                ack: content,
                            }),
                            height: Height::from_noncosmos_height(block_number),
                            tx_hash: tx.into(),
                        }
                    }
                    PacketStatus::Ack | PacketStatus::Recv => unreachable!(),
                },
            )
            .collect::<Vec<_>>();

        let tip_block_number: u64 = self
            .rpc_client
            .get_tip_header()
            .await
            .unwrap()
            .inner
            .number
            .into();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb packet events collection"),
            height: Height::from_noncosmos_height(tip_block_number),
            events,
        })
    }

    async fn search_and_extract<T, F>(
        &self,
        search_key: SearchKey,
        extractor: &F,
        limit: u32,
    ) -> Result<Vec<((T, H256), u64)>>
    where
        F: Fn(TransactionView) -> Result<(T, H256)>,
    {
        let cells = self
            .rpc_client
            .fetch_live_cells(search_key, limit, None)
            .await
            .map_err(|_| Error::collect_events_failed("fetch ibc cells failed".to_string()))?;

        let block_numbers = cells
            .objects
            .iter()
            .map(|cell| cell.block_number.into())
            .collect::<Vec<u64>>();
        let ibc_response = cells
            .objects
            .into_iter()
            .map(|cell| self.rpc_client.get_transaction(&cell.out_point.tx_hash));

        let ibc_iterator = futures::future::join_all(ibc_response)
            .await
            .into_iter()
            .zip(block_numbers)
            .filter_map(|(tx, block_number)| {
                if let Ok(Some(tx)) = tx {
                    if tx.tx_status.status == Status::Committed && tx.transaction.is_some() {
                        return Some((tx.transaction.unwrap(), block_number));
                    }
                }
                None
            });

        let mut result = vec![];
        for (tx, block_number) in ibc_iterator {
            let tx = match tx.inner {
                ckb_jsonrpc_types::Either::Left(tx) => tx,
                ckb_jsonrpc_types::Either::Right(json) => {
                    serde_json::from_slice(json.as_bytes()).unwrap()
                }
            };
            result.push((extractor(tx)?, block_number));
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
