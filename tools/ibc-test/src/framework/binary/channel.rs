use std::cell::RefCell;

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

use crate::framework::utils::common::{get_chain_type, transfer_port_id};

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
    overrides: ExtendedChannelOverrides,
    test: &'a Test,
}

impl<'a, Test> RunExtendedChannelTest<'a, Test> {
    fn new(test: &'a Test) -> Self {
        Self {
            test,
            overrides: ExtendedChannelOverrides::default(),
        }
    }
}

impl<'a, Test> HasOverrides for RunExtendedChannelTest<'a, Test> {
    type Overrides = ExtendedChannelOverrides;

    fn get_overrides(&self) -> &Self::Overrides {
        &self.overrides
    }
}

#[derive(Default)]
pub struct ExtendedChannelOverrides {
    test_config: RefCell<Option<TestConfig>>,
}

impl TestOverrides for ExtendedChannelOverrides {
    fn modify_test_config(&self, config: &mut TestConfig) {
        *self.test_config.borrow_mut() = Some(config.to_owned());
    }

    fn channel_port_a(&self) -> PortId {
        let config_opt = self.test_config.borrow();
        let config = config_opt.as_ref().unwrap();
        let command = config.chain_command_paths.first().unwrap();
        transfer_port_id(get_chain_type(command))
    }

    fn channel_port_b(&self) -> PortId {
        let config_opt = self.test_config.borrow();
        let config = config_opt.as_ref().unwrap();
        let command = config.chain_command_paths.iter().last().unwrap();
        transfer_port_id(get_chain_type(command))
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
        self.test.run(config, relayer, chains.clone(), channel)?;
        log::info!("check monitor threads are still working");
        let _ = chains.handle_a().subscribe().map_err(Error::relayer)?;
        let _ = chains.handle_b().subscribe().map_err(Error::relayer)?;
        Ok(())
    }
}
