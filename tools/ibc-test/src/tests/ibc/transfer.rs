use ibc_test_framework::{chain::chain_type::ChainType, prelude::*};

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
        _relayer: RelayerDriver,
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

        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().relayer().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let amount = 100u64;

        let packet = chains.node_a.chain_driver().ibc_transfer_token(
            &channel.port_a.as_ref(),
            &channel.channel_id_a.as_ref(),
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &denom_a.with_amount(amount).as_ref(),
        )?;
        assert_eq!(&packet.destination_channel, channel.channel_id_b.value());
        assert_eq!(&packet.destination_port, channel.port_b.value());

        Ok(())
    }
}
