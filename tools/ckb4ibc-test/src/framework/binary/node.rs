use ibc_test_framework::{
    chain::builder::ChainBuilder,
    framework::{
        base::BasicTest,
        binary::node::{NodeConfigOverride, NodeGenesisOverride},
    },
    prelude::*,
};

use crate::framework::bootstrap::ckb::bootstrap_single_ckb_node;

/**
   A wrapper type that lifts a test case that implements [`BinaryNodeTest`]
   into a test case that implements [`BasicTest`].
*/
pub struct RunArbitraryBinaryNodeTest<'a, Test> {
    /// Inner test
    pub test: &'a Test,
}

impl<'a, Test, Overrides> BasicTest for RunArbitraryBinaryNodeTest<'a, Test>
where
    Test: BinaryNodeTest,
    Test: HasOverrides<Overrides = Overrides>,
    Overrides: NodeConfigOverride + NodeGenesisOverride,
{
    fn run(&self, config: &TestConfig, builder: &ChainBuilder) -> Result<(), Error> {
        let (node_a, _miner_a) = bootstrap_single_ckb_node(
            builder,
            "0",
            config.bootstrap_with_random_ids,
            |config| self.test.get_overrides().modify_node_config(config),
            |genesis| self.test.get_overrides().modify_genesis_file(genesis),
            0,
        )?;

        let (node_b, _miner_b) = bootstrap_single_ckb_node(
            builder,
            "1",
            config.bootstrap_with_random_ids,
            |config| self.test.get_overrides().modify_node_config(config),
            |genesis| self.test.get_overrides().modify_genesis_file(genesis),
            1,
        )?;

        let _node_process_a = node_a.process.clone();
        let _node_process_b = node_b.process.clone();

        eprintln!("Node is initialized, Starting running inner test..........");
        self.test.run(config, node_a, node_b)?;

        Ok(())
    }
}

impl<'a, Test, Overrides> HasOverrides for RunArbitraryBinaryNodeTest<'a, Test>
where
    Test: HasOverrides<Overrides = Overrides>,
{
    type Overrides = Overrides;

    fn get_overrides(&self) -> &Self::Overrides {
        self.test.get_overrides()
    }
}
