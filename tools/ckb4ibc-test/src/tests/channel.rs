use ckb_types::H256;
use ibc_test_framework::prelude::*;
use std::str::FromStr;

use crate::framework::utils::ckb::*;

pub struct CKB4IbcChannelTest<'a, Test> {
    /// Inner test
    pub test: &'a Test,
}

impl<'a, Test> CKB4IbcChannelTest<'a, Test> {
    pub fn new(test: &'a Test) -> Self {
        Self { test }
    }
}

impl<'a, Test, Overrides> HasOverrides for CKB4IbcChannelTest<'a, Test>
where
    Test: HasOverrides<Overrides = Overrides>,
{
    type Overrides = Overrides;

    fn get_overrides(&self) -> &Self::Overrides {
        self.test.get_overrides()
    }
}

impl<'a, Test> BinaryChannelTest for CKB4IbcChannelTest<'a, Test>
where
    Test: BinaryChannelTest,
{
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "check conneciton and channel on-chain status ({}: {}/{}, {}: {}/{})",
            chains.chain_id_a(),
            channels.connection.connection_id_a,
            channels.channel_id_a,
            chains.chain_id_b(),
            channels.connection.connection_id_b,
            channels.channel_id_b,
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
        let port_a = H256::from_str(channels.port_a.value().to_string().as_str())
            .unwrap()
            .into();
        let port_b = H256::from_str(channels.port_b.value().to_string().as_str())
            .unwrap()
            .into();
        let a_channel = fetch_ibc_channel_cell(rpc_port_a, port_a, channels.channel_id_a.value());
        let b_channel = fetch_ibc_channel_cell(rpc_port_b, port_b, channels.channel_id_b.value());

        if !check_channel(&a_channel) || !check_channel(&b_channel) {
            panic!("create channel failed")
        }

        self.test.run(config, relayer, chains, channels)
    }
}
