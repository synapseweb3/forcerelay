use std::sync::Arc;

use crate::framework::utils::{axon::read_deployed_contracts, common::wait_task};
use ethers::{
    prelude::{k256::ecdsa::SigningKey, Wallet, *},
    providers::{Middleware, Provider, Ws},
};
use ibc_solidity_abi::generated::mock_transfer::MockTransfer;
use ibc_test_framework::{chain::chain_type::ChainType, prelude::*};
use relayer::{
    config::axon::AxonChainConfig, ibc_contract::OwnableIBCHandlerEvents, keyring::Secp256k1KeyPair,
};

pub struct TransferTest;

impl TransferTest {
    pub fn new() -> Self {
        Self
    }
}

impl TestOverrides for TransferTest {}

impl BinaryChannelTest for TransferTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        // TODO support CKB

        let chain_a = &chains.node_a.chain_driver().value().chain_type;
        let chain_b = &chains.node_b.chain_driver().value().chain_type;
        if chain_a != &ChainType::Axon || chain_b != &ChainType::Axon {
            log::warn!("Ignore transfer test for chain ({chain_a:?},{chain_b:?})");
            return Ok(());
        }

        wait_task(send_transfer(&relayer, &chains, &channel)).expect("failed to send transfer");

        Ok(())
    }
}

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

async fn new_mock_contract<H: ChainHandle>(
    client: Provider<Ws>,
    chain_handler: H,
    mock_transfer_address: H160,
) -> eyre::Result<MockTransfer<SignerMiddleware<Provider<Ws>, Wallet<SigningKey>>>> {
    let key_entry = chain_handler
        .get_key()?
        .downcast::<Secp256k1KeyPair>()
        .expect("wrong key");
    let chain_id: u64 = client.get_chainid().await?.as_u64();
    let wallet = key_entry.into_ether_wallet().with_chain_id(chain_id);
    let client = Arc::new(SignerMiddleware::new(client.clone(), wallet));
    Ok(MockTransfer::new(mock_transfer_address, client))
}

async fn send_transfer<ChainA: ChainHandle, ChainB: ChainHandle>(
    relayer: &RelayerDriver,
    chains: &ConnectedChains<ChainA, ChainB>,
    channel: &ConnectedChannel<ChainA, ChainB>,
) -> eyre::Result<()> {
    let client_a = {
        let config_a = get_chain_config(relayer, chains.node_a.chain_id().value());
        let url = config_a.websocket_addr.to_string();
        Provider::connect(url).await?
    };
    let client_b = {
        let config_b = get_chain_config(relayer, chains.node_b.chain_id().value());
        let url = config_b.websocket_addr.to_string();
        Provider::connect(url).await?
    };
    let denom = "AT".to_string();
    let amount = 100;
    let timeout_height = 0;
    let source_port = channel.port_a.to_string();
    let source_channel = channel.channel_id_a.to_string();
    let (mock_transfer_address, ibc_handler_address) = {
        let c = read_deployed_contracts(&chains.node_a.chain_driver().value().home_path)
            .expect("failed to fetch deployed contracts");
        (c.mock_transfer_contract_address, c.contract_address)
    };
    let (receiver, receiver_ibc_handler_address) = {
        let c = read_deployed_contracts(&chains.node_b.chain_driver().value().home_path)
            .expect("failed to fetch deployed contracts");
        (c.mock_transfer_contract_address, c.contract_address)
    };

    let contract = new_mock_contract(
        client_a.clone(),
        relayer.registry.get_or_spawn(chains.chain_id_a().value())?,
        mock_transfer_address,
    )
    .await?;
    let a_chain_tip = client_a.get_block_number().await?;
    let b_chain_tip = client_b.get_block_number().await?;

    let tx = contract.send_transfer(
        denom,
        amount,
        receiver,
        source_port,
        source_channel,
        timeout_height,
    );
    let pending_tx = tx.send().await?;
    pending_tx.await?;

    // check send packet event
    {
        let filter = Filter::new()
            .address(ibc_handler_address)
            .from_block(a_chain_tip);
        let logs = client_a.get_logs(&filter).await?;
        assert_eq!(logs.len(), 1);

        let event = OwnableIBCHandlerEvents::decode_log(&logs[0].clone().into())?;
        log::info!("chain_a packet log: {event:?}");
        assert!(matches!(
            event,
            OwnableIBCHandlerEvents::SendPacketFilter(..)
        ));
    }

    // check receive packet event
    let event = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let filter = Filter::new()
                .address(receiver_ibc_handler_address)
                .from_block(b_chain_tip);
            let logs = client_b.get_logs(&filter).await?;
            for log in logs.into_iter() {
                let event = OwnableIBCHandlerEvents::decode_log(&log.into()).unwrap();

                if matches!(event, OwnableIBCHandlerEvents::ReceivePacketFilter(..)) {
                    return eyre::Result::<_, ProviderError>::Ok(event);
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await??;

    log::info!("chain_b packet log: {event:?}");
    Ok(())
}
