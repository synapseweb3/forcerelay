use ibc_test_framework::prelude::*;
use log::{debug, info};
use tokio::runtime::Runtime;

mod utils;
use utils::*;

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
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        println!("\n============== Start Packet Test Over Channel ============\n");
        info!(
            "send sudt packets over channel (chain_a {}: {}/{}, chain_b {}: {}/{})",
            chains.chain_id_a(),
            channels.port_a,
            channels.channel_id_a,
            chains.chain_id_b(),
            channels.port_b,
            channels.channel_id_b,
        );
        let rt = Runtime::new()?;

        // 1. prepare essential variables and check wallet balances
        let (chain_a_config, chain_a_url, chain_a_signer) = prepare_artificials(
            &relayer.config,
            chains.handle_a(),
            channels.channel_id_a.value(),
        )?;
        let (chain_b_config, chain_b_url, chain_b_signer) = prepare_artificials(
            &relayer.config,
            chains.handle_b(),
            channels.channel_id_b.value(),
        )?;
        info!(
            "relayer wallet balance: {} CKB on chain_a, {} CKB on chain_b",
            wallet_balance(&rt, &chain_a_url, &chain_a_config.user_lock_script())?,
            wallet_balance(&rt, &chain_b_url, &chain_b_config.user_lock_script())?
        );

        // 2. trigger SendPacket event on ChainA
        debug!("send send_packet transaction to chain_a");
        let message = b"ping".to_vec();
        let send_packet_tx = generate_send_packet_transaction(
            &rt,
            &chain_a_config,
            &chain_a_url,
            &chain_a_signer,
            channels.port_b.to_string(),
            channels.channel_id_b.to_string(),
            message,
        )?;
        let hash = send_transaction(&chain_a_url, send_packet_tx)?;
        info!(
            "🍻 successfully sent send_packet transaction to chain_a, hash = {}",
            hex::encode(hash)
        );

        // 3. listen RecvPacket event on ChainB
        debug!("wait recv_packet being found on chain_b");
        let mut recv_packets =
            listen_and_wait_packet_cells(&rt, &chain_b_url, &chain_b_config, |packet| {
                packet.is_recv_packet()
            })?;
        if recv_packets.is_empty() {
            return Err(eyre!("not found recv packet on chain_b {}", chains.chain_id_b()).into());
        };
        info!("🍻 successfully find recv_packet cell on chain_b");

        // 4. trigger WriteAck event on ChainB
        debug!("send write_ack transaction to chain_b");
        let acknowledgemnt = b"pong".to_vec();
        let write_ack_tx = generate_write_ack_transaction(
            &rt,
            &chain_b_config,
            &chain_b_url,
            &chain_b_signer,
            recv_packets.remove(0),
            acknowledgemnt,
        )?;
        let hash = send_transaction(&chain_b_url, write_ack_tx)?;
        info!(
            "🍻 successfully sent write_ack transaction to chain_b, hash = {}",
            hex::encode(hash)
        );

        // 5. lisen AckPacket event on ChainA
        debug!("wait ack_packet being found on chain_a");
        let mut ack_packets =
            listen_and_wait_packet_cells(&rt, &chain_a_url, &chain_a_config, |packet| {
                packet.is_ack_packet()
            })?;
        if ack_packets.is_empty() {
            return Err(eyre!("not found ack packet on chain_a {}", chains.chain_id_a()).into());
        };
        info!("🍻 successfully find ack_packet cell on chain_a");

        // 6. comsune AckPacket cell on ChainA
        debug!("send ack_packet consume transaction to chain_a");
        let consume_ack_packet_tx = generate_consume_ack_packet_transaction(
            &rt,
            &chain_a_config,
            &chain_a_url,
            &chain_a_signer,
            ack_packets.remove(0),
        )?;
        let hash = send_transaction(&chain_a_url, consume_ack_packet_tx)?;
        info!(
            "🍻 successfully consumed ack_packet on chain_a, hash = {}",
            hex::encode(hash)
        );

        Ok(())
    }
}
