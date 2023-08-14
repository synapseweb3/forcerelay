use std::str::FromStr;

use crate::framework::{binary::channel::run_arbitrary_binary_channel_test, utils::ckb::*};
use ckb_types::H256;
use ibc_test_framework::prelude::*;

#[test]
fn test_ckb4ibc_channel() -> Result<(), Error> {
    run_arbitrary_binary_channel_test(&CKB4IbcChannelTest)
}

/// CKB only allow h256 as portId
fn transfer_port_id() -> PortId {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"transfer");
    PortId::from_str(H256::from(buf).to_string().as_str()).unwrap()
}

pub struct CKB4IbcChannelTest;

impl TestOverrides for CKB4IbcChannelTest {
    fn channel_port_a(&self) -> PortId {
        transfer_port_id()
    }

    fn channel_port_b(&self) -> PortId {
        transfer_port_id()
    }
}

impl BinaryChannelTest for CKB4IbcChannelTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "successfully create channel from chain {} conn {} port {} to chain {} conn {} port {}",
            chains.chain_id_a(),
            channel.channel_id_a,
            channel.port_a,
            chains.chain_id_b(),
            channel.channel_id_b,
            channel.port_b,
        );
        let rpc_port_a = chains.node_a.chain_driver().value().rpc_port.into();
        let rpc_port_b = chains.node_b.chain_driver().value().rpc_port.into();

        // check connection
        let a_connection = fetch_ibc_connections(rpc_port_a);
        let b_connection = fetch_ibc_connections(rpc_port_b);

        // IBC test-framework will open a dummy connection on chain B before the test started, so chain B has two connections
        if !check_ibc_connection(a_connection, 1) || !check_ibc_connection(b_connection, 2) {
            panic!("create connection failed");
        }

        // check channel
        let port_a = H256::from_str(channel.port_a.into_value().to_string().as_str())
            .unwrap()
            .into();
        let port_b = H256::from_str(channel.port_b.into_value().to_string().as_str())
            .unwrap()
            .into();
        let a_channel =
            fetch_ibc_channel_cell(rpc_port_a, port_a, &channel.channel_id_a.into_value());
        let b_channel =
            fetch_ibc_channel_cell(rpc_port_b, port_b, &channel.channel_id_b.into_value());

        if !check_channel(&a_channel) || !check_channel(&b_channel) {
            panic!("create channel failed")
        }

        Ok(())
    }
}
