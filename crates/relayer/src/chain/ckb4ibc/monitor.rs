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
    AcknowledgePacket, OpenInit as ChannelOpenInit, OpenTry as ChannelOpenTry, ReceivePacket,
    SendPacket,
};
use ibc_relayer_types::core::ics04_channel::packet::{Packet, Sequence};
use ibc_relayer_types::core::ics04_channel::timeout::TimeoutHeight;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::timestamp::Timestamp;
use tokio::runtime::Runtime as TokioRuntime;

use crate::chain::ckb::prelude::CkbReader;
use crate::chain::ckb::rpc_client::RpcClient;
use crate::chain::ckb4ibc::extractor::{
    extract_channel_end_from_tx, extract_ibc_connections_from_tx, extract_ibc_packet_from_tx,
};
use crate::chain::tracking::TrackingId;
use crate::config::ckb4ibc::ChainConfig;
use crate::event::bus::EventBus;
use crate::event::monitor::{Error, EventBatch, MonitorCmd, Next, Result, TxMonitorCmd};
use crate::event::IbcEventWithHeight;

use super::cache_set::CacheSet;
use super::utils::{get_script_hash, get_search_key};

// todo add cell emitter here
pub struct Ckb4IbcEventMonitor {
    rt: Arc<TokioRuntime>,
    rpc_client: Arc<RpcClient>,
    rx_cmd: Receiver<MonitorCmd>,
    event_bus: EventBus<Arc<Result<EventBatch>>>,
    config: ChainConfig,
    cache_set: RwLock<CacheSet<H256>>,
}

impl Ckb4IbcEventMonitor {
    pub fn new(
        rt: Arc<TokioRuntime>,
        rpc_client: Arc<RpcClient>,
        config: ChainConfig,
    ) -> (Self, TxMonitorCmd) {
        let (tx_cmd, rx_cmd) = crossbeam_channel::unbounded();
        let monitor = Ckb4IbcEventMonitor {
            rt,
            rpc_client,
            rx_cmd,
            event_bus: EventBus::default(),
            config,
            cache_set: RwLock::new(CacheSet::new(512)),
        };
        (monitor, TxMonitorCmd::new(tx_cmd))
    }

    pub fn run(mut self) {
        let rt = self.rt.clone();
        loop {
            std::thread::sleep(Duration::from_secs(5));
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
        let result = async {
            tokio::select! {
                Ok(batch) = self.fetch_channel_events() => {
                    batch
                },
                Ok(batch) = self.fetch_connection_events() => {
                    batch
                },
                Ok(batch) = self.fetch_packet_events() => {
                    batch
                }
            }
        }
        .await;

        self.process_batch(result);
        Next::Continue
    }

    async fn fetch_connection_events(&self) -> Result<EventBatch> {
        let connection_code_hash = get_script_hash(&self.config.connection_type_args);
        let script = Script::new_builder()
            .code_hash(connection_code_hash)
            .hash_type(ScriptHashType::Type.into())
            .args(
                self.config
                    .lc_client_id_bytes(ClientType::Ckb4Ibc)
                    .unwrap()
                    .as_slice()
                    .pack(),
            )
            .build();
        let key = get_search_key(script);
        let (ibc_connection_cell, tx_hash) = self
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
                height: Height::default(),
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
                    let attrs = Attributes {
                        connection_id: Some(ConnectionId::from_str(&idx.to_string()).unwrap()), // todo connection id here is invalid
                        client_id: self.config.lc_client_id(ClientType::Ckb4Ibc).unwrap(),
                        counterparty_connection_id: None,
                        counterparty_client_id: ClientId::from_str(
                            &connection_end.counterparty.client_id,
                        )
                        .unwrap(),
                    };
                    let event = IbcEvent::OpenInitConnection(ConnectionOpenInit(attrs));
                    Some(IbcEventWithHeight {
                        event,
                        height: Height::default(),
                        tx_hash: tx_hash.clone().into(),
                    })
                }
                CkbState::OpenTry => {
                    let attrs = Attributes {
                        connection_id: Some(ConnectionId::from_str(&idx.to_string()).unwrap()), // todo connection id here is invalid
                        client_id: self.config.lc_client_id(ClientType::Ckb4Ibc).unwrap(),
                        counterparty_connection_id: None,
                        counterparty_client_id: ClientId::from_str(
                            &connection_end.counterparty.client_id,
                        )
                        .unwrap(),
                    };
                    let event = IbcEvent::OpenTryConnection(ConnectionOpenTry(attrs));
                    Some(IbcEventWithHeight {
                        event,
                        height: Height::default(),
                        tx_hash: tx_hash.clone().into(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb connection events collection"),
            height: Height::default(),
            events,
        })
    }

    async fn fetch_channel_events(&self) -> Result<EventBatch> {
        let script = Script::new_builder()
            .code_hash(get_script_hash(&self.config.channel_type_args))
            .args(
                ChannelArgs {
                    client_id: self.config.lc_client_id_bytes(ClientType::Ckb4Ibc).unwrap(),
                    open: false,
                    channel_id: Default::default(),
                    port_id: Default::default(),
                }
                .get_prefix_for_searching_unopen()
                .pack(),
            )
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
                20,
            )
            .await?;

        let events = identified_channel_ends
            .into_iter()
            .filter(|(_, tx)| !self.cache_set.read().unwrap().has(tx))
            .map(|(channel_end, tx)| {
                self.cache_set.write().unwrap().insert(tx.clone());
                (channel_end, tx)
            })
            .map(|item| match item.0.channel_end.state {
                State::Init => IbcEventWithHeight {
                    event: IbcEvent::OpenInitChannel(ChannelOpenInit {
                        port_id: item.0.port_id,
                        channel_id: Some(item.0.channel_id),
                        connection_id: item.0.channel_end.connection_hops[0].clone(),
                        counterparty_port_id: item.0.channel_end.remote.port_id,
                        counterparty_channel_id: item.0.channel_end.remote.channel_id,
                    }),
                    height: Height::default(),
                    tx_hash: item.1.into(),
                },
                State::TryOpen => IbcEventWithHeight {
                    event: IbcEvent::OpenTryChannel(ChannelOpenTry {
                        port_id: item.0.port_id,
                        channel_id: Some(item.0.channel_id),
                        connection_id: item.0.channel_end.connection_hops[0].clone(),
                        counterparty_port_id: item.0.channel_end.remote.port_id,
                        counterparty_channel_id: item.0.channel_end.remote.channel_id,
                    }),
                    height: Height::default(),
                    tx_hash: item.1.into(),
                },
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb channel events collection"),
            height: Height::default(),
            events,
        })
    }

    async fn fetch_packet_events(&self) -> Result<EventBatch> {
        let script = Script::new_builder()
            .code_hash(get_script_hash(&self.config.packet_type_args))
            .args("".pack())
            .build();
        let key = get_search_key(script);
        let ibc_packets = self
            .search_and_extract(
                key,
                &|tx| {
                    let hash = tx.hash.clone();
                    let obj = extract_ibc_packet_from_tx(tx)
                        .map_err(|_| Error::collect_events_failed("packet".to_string()))?;
                    Ok((obj, hash))
                },
                20,
            )
            .await?;
        let events = ibc_packets
            .into_iter()
            .filter(|(packet, tx)| {
                packet.status != PacketStatus::Ack && !self.cache_set.read().unwrap().has(tx)
            })
            .map(|(packet, tx)| {
                self.cache_set.write().unwrap().insert(tx.clone());
                (packet, tx)
            })
            .map(|item| match item.0.status {
                PacketStatus::Send => IbcEventWithHeight {
                    event: IbcEvent::SendPacket(SendPacket {
                        packet: convert_packet(item.0),
                    }),
                    height: Height::default(),
                    tx_hash: item.1.into(),
                },
                PacketStatus::Recv => IbcEventWithHeight {
                    event: IbcEvent::ReceivePacket(ReceivePacket {
                        packet: convert_packet(item.0),
                    }),
                    height: Height::default(),
                    tx_hash: item.1.into(),
                },
                PacketStatus::InboxAck => IbcEventWithHeight {
                    event: IbcEvent::AcknowledgePacket(AcknowledgePacket {
                        packet: convert_packet(item.0),
                    }),
                    height: Height::default(),
                    tx_hash: item.1.into(),
                },
                PacketStatus::OutboxAck => todo!(),
                PacketStatus::Ack => unreachable!(),
            })
            .collect::<Vec<_>>();
        Ok(EventBatch {
            chain_id: self.config.id.clone(),
            tracking_id: TrackingId::Static("ckb channel events collection"),
            height: Height::default(),
            events,
        })
    }

    async fn search_and_extract<T, F>(
        &self,
        search_key: SearchKey,
        extractor: &F,
        limit: u32,
    ) -> Result<Vec<(T, H256)>>
    where
        F: Fn(TransactionView) -> Result<(T, H256)>,
    {
        let cells = self
            .rpc_client
            .fetch_live_cells(search_key, limit, None)
            .await
            .map_err(|_| Error::collect_events_failed("fetch channel event failed".to_string()))?;

        let tx_response = cells
            .objects
            .into_iter()
            .map(|cell| self.rpc_client.get_transaction(&cell.out_point.tx_hash));

        let result = futures::future::join_all(tx_response)
            .await
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
                extractor(tx)
            })
            .collect::<Vec<_>>();

        Ok(result)
    }

    fn process_batch(&mut self, batch: EventBatch) {
        self.event_bus.broadcast(Arc::new(Ok(batch)));
    }
}

fn convert_packet(packet: IbcPacket) -> Packet {
    let sequence = Sequence::from(packet.packet.sequence as u64);

    let source_port = {
        let s = &packet.packet.source_port_id;
        PortId::from_str(s).unwrap()
    };

    let destination_port = {
        let s = &packet.packet.destination_port_id;
        PortId::from_str(s).unwrap()
    };

    let source_channel = {
        let s = &packet.packet.source_channel_id;
        ChannelId::from_str(s).unwrap()
    };

    let destination_channel = {
        let s = &packet.packet.destination_channel_id;
        ChannelId::from_str(s).unwrap()
    };

    Packet {
        sequence,
        source_port,
        source_channel,
        destination_port,
        destination_channel,
        data: packet.packet.data,
        timeout_height: TimeoutHeight::Never,
        timeout_timestamp: Timestamp::none(),
    }
}
