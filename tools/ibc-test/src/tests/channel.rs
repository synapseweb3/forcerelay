use std::str::FromStr;

use crate::framework::utils::ckb::*;
use ckb_types::H256;
use ibc_test_framework::{chain::chain_type::ChainType, prelude::*};

pub struct ChannelTest;

impl TestOverrides for ChannelTest {}

impl ChannelTest {
    pub fn new() -> Self {
        Self
    }
}

fn check_ckb_ibc_cells(
    rpc_port: u16,
    expected_connections: u32,
    port: &PortId,
    channel_id: &ChannelId,
) -> Result<(), Error> {
    // check connection
    let connection = fetch_ibc_connections(rpc_port.into());

    // IBC test-framework will open a dummy connection on chain B before the test started, so chain B has two connections
    if !check_ibc_connection(connection, expected_connections) {
        panic!("create connection failed");
    }

    // check channel
    let port = H256::from_str(port.to_string().as_str()).unwrap().into();
    let channel = fetch_ibc_channel_cell(rpc_port.into(), port, channel_id);

    if !check_channel(&channel) {
        panic!("create channel failed")
    }

    Ok(())
}

impl BinaryChannelTest for ChannelTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "check conneciton and channel on-chain status ({}: {}/{}, {}: {}/{})",
            chains.chain_id_a(),
            channel.connection.connection_id_a,
            channel.channel_id_a,
            chains.chain_id_b(),
            channel.connection.connection_id_b,
            channel.channel_id_b,
        );

        match &chains.node_a.chain_driver().value().chain_type {
            &ChainType::Ckb => {
                check_ckb_ibc_cells(
                    chains.node_a.chain_driver().value().rpc_port,
                    1,
                    &channel.port_a.clone().into_value(),
                    &channel.channel_id_a.clone().into_value(),
                )?;
            }
            chain => {
                warn!("Skip IBC channel check for chain-A({:?})", chain);
            }
        }

        match &chains.node_b.chain_driver().value().chain_type {
            &ChainType::Ckb => {
                check_ckb_ibc_cells(
                    chains.node_b.chain_driver().value().rpc_port,
                    2,
                    &channel.port_b.clone().into_value(),
                    &channel.channel_id_b.clone().into_value(),
                )?;
            }
            chain => {
                warn!("Skip IBC channel check for chain-B({:?})", chain);
            }
        }
        Ok(())
    }
}
