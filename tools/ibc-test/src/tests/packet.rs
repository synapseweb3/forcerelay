use ckb_types::H256;
use ibc_test_framework::prelude::*;
use std::str::FromStr;

/// CKB only allow h256 as portId
fn transfer_port_id() -> PortId {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"transfer");
    PortId::from_str(H256::from(buf).to_string().as_str()).unwrap()
}

pub struct CKB4IbcPacketTest {}

impl TestOverrides for CKB4IbcPacketTest {
    fn channel_port_a(&self) -> PortId {
        transfer_port_id()
    }

    fn channel_port_b(&self) -> PortId {
        transfer_port_id()
    }
}

impl BinaryChannelTest for CKB4IbcPacketTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "sending sUDT packets over channel ({}: {}/{}, {}: {}/{})",
            chains.chain_id_a(),
            channel.port_a,
            channel.channel_id_a,
            chains.chain_id_b(),
            channel.port_b,
            channel.channel_id_b,
        );

        Ok(())
    }
}
