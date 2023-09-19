//! Implement ibc transfer with MockTransfer contract

use std::{path::Path, str::FromStr, sync::Arc};

use crate::{prelude::*, types::axon::DeployedContracts};
use ethers::{
    prelude::{k256::ecdsa::SigningKey, Wallet, *},
    providers::{Middleware, Provider, Ws},
};
use eyre::eyre;
use http::Uri;
use ibc_relayer::{
    chain::{cosmos::types::config::TxConfig, SEC_TO_NANO},
    config::axon::AxonChainConfig,
    event::IbcEventWithHeight,
    ibc_contract::OwnableIBCHandlerEvents,
    keyring::Secp256k1KeyPair,
};
use ibc_relayer_types::{
    core::{
        ics02_client::{client_type::ClientType, events::CreateClient},
        ics04_channel::{events::ReceivePacket, packet::Packet},
    },
    events::IbcEvent,
    Height,
};
use ibc_solidity_abi::generated::mock_transfer::MockTransfer;

fn get_chain_config(relayer: &RelayerDriver, chain_id: &ChainId) -> AxonChainConfig {
    relayer
        .config
        .chains
        .iter()
        .find(|chain| chain.id() == chain_id)
        .cloned()
        .unwrap()
        .downcast_axon()
}

async fn new_mock_contract(
    client: Provider<Ws>,
    key_pair: &Secp256k1KeyPair,
    mock_transfer_address: H160,
) -> eyre::Result<MockTransfer<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>> {
    let chain_id: u64 = client.get_chainid().await?.as_u64();
    let wallet = key_pair.clone().into_ether_wallet().with_chain_id(chain_id);
    let client = Arc::new(SignerMiddleware::new(client.clone(), wallet));
    Ok(MockTransfer::new(mock_transfer_address, client))
}

pub fn read_deployed_contracts<P: AsRef<Path>>(chain_dir: P) -> Result<DeployedContracts, Error> {
    const AXON_CONTRACTS_CONFIG_PATH: &str = "deployed_contracts.toml";

    let path = chain_dir.as_ref().join(AXON_CONTRACTS_CONFIG_PATH);
    let content = std::fs::read_to_string(path)?;
    let c = toml::from_str(&content).map_err(|err| eyre::eyre!(err))?;
    Ok(c)
}

/// ibc token transfer
pub async fn ibc_token_transfer<SrcChain, DstChain>(
    websocket_addr: String,
    home_path: String,
    port_id: &TaggedPortIdRef<'_, SrcChain, DstChain>,
    channel_id: &TaggedChannelIdRef<'_, SrcChain, DstChain>,
    sender: &MonoTagged<SrcChain, &crate::types::wallet::Wallet>,
    recipient: &MonoTagged<DstChain, &WalletAddress>,
    token: &TaggedTokenRef<'_, SrcChain>,
    timeout: Option<Duration>,
) -> Result<Packet, Error> {
    // we set ws port on the next port of rpc port in `ibc-test/src/framework/bootstrap/node.rs`
    let client = Provider::connect(websocket_addr)
        .await
        .map_err(|err| eyre!(err))?;
    // get ibc contract address
    let (mock_transfer_address, ibc_handler_address) = {
        let c = read_deployed_contracts(&home_path).expect("failed to fetch deployed contracts");
        (c.mock_transfer_contract_address, c.contract_address)
    };
    let contract =
        new_mock_contract(client.clone(), &sender.value().key, mock_transfer_address).await?;

    let receiver = {
        let slice = hex::decode(recipient.value().as_str().trim_end_matches("0x"))
            .map_err(|err| eyre!(err))?;
        H160::from_slice(&slice)
    };
    let denom = token.denom().value().to_string();
    let amount = token.amount().0.as_u64();
    let timeout_height = timeout.map(|d| d.as_secs() / 8).unwrap_or_default();
    let tx = contract.send_transfer(
        denom,
        amount,
        receiver,
        port_id.to_string(),
        channel_id.to_string(),
        timeout_height,
    );
    let pending_tx = tx.send().await.map_err(|err| eyre!(err))?;
    let receipt_opt = pending_tx.await.map_err(|err| eyre!(err))?;
    let receipt = receipt_opt.ok_or(eyre!("axon send ibc transfer no receipt"))?;
    let events = fetch_all_ibc_events_from_tx_receipt(receipt)?;
    let packet = events
        .into_iter()
        .find_map(|event| match event.event {
            IbcEvent::SendPacket(ev) => Some(ev.packet),
            _ => None,
        })
        .ok_or_else(|| eyre!("failed to find send packet event"))?;
    Ok(packet)
}

pub fn fetch_all_ibc_events_from_tx_receipt(
    receipt: TransactionReceipt,
) -> Result<Vec<IbcEventWithHeight>, eyre::Error> {
    let block_number = receipt
        .block_number
        .expect("receipt has no block_number")
        .as_u64();
    let height = Height::from_noncosmos_height(block_number);
    let tx_hash = receipt.transaction_hash;
    // check send packet event
    let events = receipt
        .logs
        .iter()
        .filter_map(|log| {
            let event = OwnableIBCHandlerEvents::decode_log(&log.clone().into())
                .expect("decode ibc events");
            ibc_event_from_ibc_handler_event(height, tx_hash.into(), event).transpose()
        })
        .collect::<Result<_, eyre::Error>>()?;
    Ok(events)
}

fn ibc_event_from_ibc_handler_event(
    height: Height,
    tx_hash: [u8; 32],
    event: OwnableIBCHandlerEvents,
) -> Result<Option<IbcEventWithHeight>, eyre::Error> {
    use ibc_relayer::chain::axon::contract;
    use ibc_relayer_types::core::ics02_client::events as clients;
    use ibc_relayer_types::core::ics03_connection::events as connections;
    use ibc_relayer_types::core::ics04_channel::events as channels;
    use ibc_relayer_types::timestamp::*;

    let event: IbcEvent = match event {
        OwnableIBCHandlerEvents::AcknowledgePacketFilter(data) => {
            let contract::PacketData {
                sequence,
                source_port,
                source_channel,
                destination_port,
                destination_channel,
                data,
                timeout_height,
                timeout_timestamp,
            } = data.packet;

            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;

            let packet = Packet {
                sequence: sequence.into(),
                source_port: PortId::from_str(&source_port)?,
                source_channel: ChannelId::from_str(&source_channel)?,
                destination_port: PortId::from_str(&destination_port)?,
                destination_channel: ChannelId::from_str(&destination_channel)?,
                data: data.0.to_vec(),
                timeout_height: timeout_height.into(),
                timeout_timestamp: Timestamp::from_nanoseconds(timeout_timestamp * SEC_TO_NANO)?,
            };
            IbcEvent::AcknowledgePacket(channels::AcknowledgePacket { packet })
        }
        OwnableIBCHandlerEvents::CloseConfirmChannelFilter(data) => {
            let contract::CloseConfirmChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::CloseConfirmChannel(channels::CloseConfirm {
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                port_id: PortId::from_str(&port_id)?,
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::CloseInitChannelFilter(data) => {
            let contract::CloseInitChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::CloseInitChannel(channels::CloseInit {
                port_id: PortId::from_str(&port_id)?,
                channel_id: ChannelId::from_str(&channel_id)?,
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::CreateClientFilter(data) => {
            let contract::CreateClientFilter {
                client_id,
                client_type,
            } = data;
            IbcEvent::CreateClient(clients::CreateClient(clients::Attributes {
                client_id: ClientId::from_str(&client_id)?,
                client_type: ClientType::from_str(&client_type)?,
                consensus_height: height,
            }))
        }
        OwnableIBCHandlerEvents::OpenAckChannelFilter(data) => {
            let contract::OpenAckChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenAckChannel(channels::OpenAck {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenAckConnectionFilter(data) => {
            let contract::OpenAckConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenAckConnection(connections::OpenAck(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenConfirmChannelFilter(data) => {
            let contract::OpenConfirmChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenConfirmChannel(channels::OpenConfirm {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenConfirmConnectionFilter(data) => {
            let contract::OpenConfirmConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenConfirmConnection(connections::OpenConfirm(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenInitChannelFilter(data) => {
            let contract::OpenInitChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenInitChannel(channels::OpenInit {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenInitConnectionFilter(data) => {
            let contract::OpenInitConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenInitConnection(connections::OpenInit(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::OpenTryChannelFilter(data) => {
            let contract::OpenTryChannelFilter {
                port_id,
                channel_id,
                connection_id,
                counterparty_port_id,
                counterparty_channel_id,
            } = data;
            IbcEvent::OpenTryChannel(channels::OpenTry {
                port_id: PortId::from_str(&port_id)?,
                channel_id: Some(ChannelId::from_str(&channel_id)?),
                connection_id: ConnectionId::from_str(&connection_id)?,
                counterparty_port_id: PortId::from_str(&counterparty_port_id)?,
                counterparty_channel_id: Some(ChannelId::from_str(&counterparty_channel_id)?),
            })
        }
        OwnableIBCHandlerEvents::OpenTryConnectionFilter(data) => {
            let contract::OpenTryConnectionFilter {
                connection_id,
                client_id,
                counterparty_connection_id,
                counterparty_client_id,
            } = data;
            IbcEvent::OpenTryConnection(connections::OpenTry(connections::Attributes {
                connection_id: Some(ConnectionId::from_str(&connection_id)?),
                client_id: ClientId::from_str(&client_id)?,
                counterparty_connection_id: Some(ConnectionId::from_str(
                    &counterparty_connection_id,
                )?),
                counterparty_client_id: ClientId::from_str(&counterparty_client_id)?,
            }))
        }
        OwnableIBCHandlerEvents::ReceivePacketFilter(data) => {
            let contract::ReceivePacketFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
            } = data;
            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;
            IbcEvent::ReceivePacket(ReceivePacket {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.into(),
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
            })
        }
        OwnableIBCHandlerEvents::SendPacketFilter(data) => {
            let contract::SendPacketFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
            } = data;
            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;
            IbcEvent::SendPacket(channels::SendPacket {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.into(),
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
            })
        }
        OwnableIBCHandlerEvents::UpdateClientFilter(data) => {
            let contract::UpdateClientFilter {
                client_id,
                client_message,
            } = data;
            let client_id = ClientId::from_str(&client_id)?;
            let client_type = ClientType::from(client_id.clone());
            IbcEvent::UpdateClient(clients::UpdateClient {
                common: clients::Attributes {
                    client_id,
                    client_type,
                    consensus_height: height,
                },
                header: None,
            })
        }
        OwnableIBCHandlerEvents::WriteAcknowledgementFilter(data) => {
            let contract::WriteAcknowledgementFilter {
                packet:
                    contract::PacketData {
                        sequence,
                        source_port,
                        source_channel,
                        destination_port,
                        destination_channel,
                        data,
                        timeout_height,
                        timeout_timestamp,
                    },
                acknowledgement,
            } = data;

            let timeout_height = Height::new(
                timeout_height.revision_number,
                timeout_height.revision_height,
            )?;
            IbcEvent::WriteAcknowledgement(channels::WriteAcknowledgement {
                packet: Packet {
                    sequence: sequence.into(),
                    source_port: PortId::from_str(&source_port)?,
                    source_channel: ChannelId::from_str(&source_channel)?,
                    destination_port: PortId::from_str(&destination_port)?,
                    destination_channel: ChannelId::from_str(&destination_channel)?,
                    data: data.to_vec(),
                    timeout_height: timeout_height.try_into()?,
                    timeout_timestamp: Timestamp::from_nanoseconds(
                        timeout_timestamp * SEC_TO_NANO,
                    )?,
                },
                ack: acknowledgement.to_vec(),
            })
        }
        OwnableIBCHandlerEvents::RegisterCellEmitterFilterFilter(_)
        | OwnableIBCHandlerEvents::RemoveCellEmitterFilterFilter(_)
        | OwnableIBCHandlerEvents::OwnershipTransferredFilter(_) => return Ok(None),
    };
    Ok(Some(IbcEventWithHeight {
        event,
        height,
        tx_hash,
    }))
}

pub async fn send_transfer<ChainA: ChainHandle, ChainB: ChainHandle>(
    relayer: &RelayerDriver,
    chains: &ConnectedChains<ChainA, ChainB>,
    channel: &ConnectedChannel<ChainA, ChainB>,
) -> eyre::Result<()> {
    unimplemented!();
    // let client_a = {
    //     let config_a = get_chain_config(relayer, chains.node_a.chain_id().value());
    //     let url = config_a.websocket_addr.to_string();
    //     Provider::connect(url).await?
    // };
    // let client_b = {
    //     let config_b = get_chain_config(relayer, chains.node_b.chain_id().value());
    //     let url = config_b.websocket_addr.to_string();
    //     Provider::connect(url).await?
    // };
    // let denom = "AT".to_string();
    // let amount = 100;
    // let timeout_height = 0;
    // let source_port = channel.port_a.to_string();
    // let source_channel = channel.channel_id_a.to_string();
    // let (mock_transfer_address, ibc_handler_address) = {
    //     let c = read_deployed_contracts(&chains.node_a.chain_driver().value().home_path)
    //         .expect("failed to fetch deployed contracts");
    //     (c.mock_transfer_contract_address, c.contract_address)
    // };
    // let (receiver, receiver_ibc_handler_address) = {
    //     let c = read_deployed_contracts(&chains.node_b.chain_driver().value().home_path)
    //         .expect("failed to fetch deployed contracts");
    //     (c.mock_transfer_contract_address, c.contract_address)
    // };

    // let contract = new_mock_contract(
    //     client_a.clone(),
    //     relayer.registry.get_or_spawn(chains.chain_id_a().value())?,
    //     mock_transfer_address,
    // )
    // .await?;
    // let a_chain_tip = client_a.get_block_number().await?;
    // let b_chain_tip = client_b.get_block_number().await?;

    // let tx = contract.send_transfer(
    //     denom,
    //     amount,
    //     receiver,
    //     source_port,
    //     source_channel,
    //     timeout_height,
    // );
    // let pending_tx = tx.send().await?;
    // pending_tx.await?;

    // // check send packet event
    // {
    //     let filter = Filter::new()
    //         .address(ibc_handler_address)
    //         .from_block(a_chain_tip);
    //     let logs = client_a.get_logs(&filter).await?;
    //     assert_eq!(logs.len(), 1);

    //     let event = OwnableIBCHandlerEvents::decode_log(&logs[0].clone().into())?;
    //     log::info!("chain_a packet log: {event:?}");
    //     assert!(matches!(
    //         event,
    //         OwnableIBCHandlerEvents::SendPacketFilter(..)
    //     ));
    // }

    // // check receive packet event
    // let event = tokio::time::timeout(Duration::from_secs(30), async {
    //     loop {
    //         let filter = Filter::new()
    //             .address(receiver_ibc_handler_address)
    //             .from_block(b_chain_tip);
    //         let logs = client_b.get_logs(&filter).await?;
    //         for log in logs.into_iter() {
    //             let event = OwnableIBCHandlerEvents::decode_log(&log.into()).unwrap();

    //             if matches!(event, OwnableIBCHandlerEvents::ReceivePacketFilter(..)) {
    //                 return eyre::Result::<_, ProviderError>::Ok(event);
    //             }
    //         }

    //         tokio::time::sleep(Duration::from_secs(1)).await;
    //     }
    // })
    // .await??;

    // log::info!("chain_b packet log: {event:?}");
    Ok(())
}
