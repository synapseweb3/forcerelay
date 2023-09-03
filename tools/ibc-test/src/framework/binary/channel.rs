use ibc_test_framework::{
    framework::{
        base::*,
        binary::{
            chain::*,
            channel::{PortsOverride, *},
            connection::*,
            node::*,
        },
        supervisor::*,
    },
    prelude::*,
};

use super::node::RunArbitraryBinaryNodeTest;

pub fn run_arbitrary_binary_node_test<Test, Overrides>(test: &Test) -> Result<(), Error>
where
    Test: BinaryNodeTest,
    Test: HasOverrides<Overrides = Overrides>,
    Overrides: NodeConfigOverride + NodeGenesisOverride + TestConfigOverride,
{
    run_basic_test(&RunArbitraryBinaryNodeTest { test })
}

pub fn run_arbitrary_binary_channel_test<Test, Overrides>(test: &Test) -> Result<(), Error>
where
    Test: BinaryChannelTest,
    Test: HasOverrides<Overrides = Overrides>,
    Overrides: TestConfigOverride
        + NodeConfigOverride
        + NodeGenesisOverride
        + RelayerConfigOverride
        + ClientOptionsOverride
        + SupervisorOverride
        + ConnectionDelayOverride
        + PortsOverride
        + ChannelOrderOverride
        + ChannelVersionOverride,
{
    run_arbitrary_binary_node_test(&RunBinaryChainTest::new(&RunBinaryConnectionTest::new(
        &RunBinaryChannelTest::new(&RunWithSupervisor::new(&RunExtendedChannelTest::new(test))),
    )))
}

/// This test override the default port for ckb and axon chain
pub struct RunExtendedChannelTest<'a, Test> {
    test: &'a Test,
}

impl<'a, Test> RunExtendedChannelTest<'a, Test> {
    fn new(test: &'a Test) -> Self {
        Self { test }
    }
}

impl<'a, Test, Overrides> HasOverrides for RunExtendedChannelTest<'a, Test>
where
    Test: HasOverrides<Overrides = Overrides>,
{
    type Overrides = Overrides;

    fn get_overrides(&self) -> &Self::Overrides {
        self.test.get_overrides()
    }
}

impl<'a, Test, Overrides> BinaryChannelTest for RunExtendedChannelTest<'a, Test>
where
    Test: BinaryChannelTest,
    Test: HasOverrides<Overrides = Overrides>,
    Overrides: PortsOverride + ChannelOrderOverride + ChannelVersionOverride,
{
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        self.test.run(config, relayer, chains, channel)
    }
}
