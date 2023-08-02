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
        &RunBinaryChannelTest::new(&RunWithSupervisor::new(test)),
    )))
}
