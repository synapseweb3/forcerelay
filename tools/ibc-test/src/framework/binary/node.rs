use ibc_test_framework::{
    chain::{builder::ChainBuilder, chain_type::ChainType},
    framework::{
        base::BasicTest,
        binary::node::{NodeConfigOverride, NodeGenesisOverride},
    },
    prelude::*,
};

use crate::framework::{
    bootstrap::node::bootstrap_single_node, utils::common::prepare_cell_emitter,
};

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
        let (node_a, _miner_a) = bootstrap_single_node(
            builder,
            "0",
            config.bootstrap_with_random_ids,
            |config| self.test.get_overrides().modify_node_config(config),
            |genesis| self.test.get_overrides().modify_genesis_file(genesis),
            0,
        )?;

        let (node_b, _miner_b) = bootstrap_single_node(
            builder,
            "1",
            config.bootstrap_with_random_ids,
            |config| self.test.get_overrides().modify_node_config(config),
            |genesis| self.test.get_overrides().modify_genesis_file(genesis),
            1,
        )?;

        let _node_process_a = node_a.process.clone();
        let _node_process_b = node_b.process.clone();

        let chain_types = (
            &node_a.chain_driver.chain_type,
            &node_b.chain_driver.chain_type,
        );
        if matches!(chain_types, (&ChainType::Axon, &ChainType::Ckb)) {
            let axon_port = node_a.chain_driver.rpc_port;
            let ckb_port = node_b.chain_driver.rpc_port;
            println!("start cell-emiter for Axon:{axon_port} and CKB:{ckb_port}");
            prepare_cell_emitter(axon_port, ckb_port)?;
        }
        if matches!(chain_types, (&ChainType::Ckb, &ChainType::Axon)) {
            let axon_port = node_b.chain_driver.rpc_port;
            let ckb_port = node_a.chain_driver.rpc_port;
            println!("start cell-emiter for Axon:{axon_port} and CKB:{ckb_port}");
            prepare_cell_emitter(axon_port, ckb_port)?;
        }

        eprintln!("Node is initialized, Starting running inner test..........");

        self.test.run(config, node_a, node_b)
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
