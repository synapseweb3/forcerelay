use ckb_sdk::unlock::SecpSighashScriptSigner;
use forcerelay_ckb_sdk::config::Config as SdkConfig;
use ibc_test_framework::prelude::*;
use log::info;
use tokio::runtime::Runtime;

pub mod utils;
use utils::*;

pub struct CKB4IbcPacketTest;

impl CKB4IbcPacketTest {
    pub fn new() -> Self {
        Self
    }
}

impl CKB4IbcPacketTest {
    #[allow(clippy::too_many_arguments)]
    fn run_instance(
        &self,
        rt: &Runtime,
        chain_a_config: &SdkConfig,
        chain_a_url: &str,
        chain_a_signer: &SecpSighashScriptSigner,
        chain_b_config: &SdkConfig,
        chain_b_url: &str,
        chain_b_signer: &SecpSighashScriptSigner,
    ) -> Result<(), Error> {
        // 2. trigger SendPacket event on ChainA
        info!("send send_packet transaction to chain_a");
        let relayer_on_a = chain_a_config.module_lock_script().calc_script_hash();
        let message = ICS20Transfer {
            denom: "AT".to_owned(),
            amount: 1000,
            sender: relayer_on_a.raw_data().to_vec(),
            receiver: relayer_on_a.raw_data().to_vec(),
        };
        let send_packet_tx = generate_send_packet_transaction(
            rt,
            chain_a_config,
            chain_a_url,
            chain_a_signer,
            &message,
        )?;
        let hash = send_transaction(chain_a_url, send_packet_tx)?;
        info!(
            "🍻 successfully sent send_packet transaction to chain_a, hash = {}",
            hex::encode(hash)
        );

        // 3. listen RecvPacket event on ChainB
        info!("wait recv_packet being found on chain_b");
        let recv_packet = listen_and_wait_packet_cell(rt, chain_b_url, chain_b_config, |packet| {
            packet.is_recv_packet()
        })?;
        let payload: ICS20Transfer =
            serde_json::from_slice(&recv_packet.packet.packet.data).expect("ics20 message");
        let relayer_on_b = chain_b_config.module_lock_script().calc_script_hash();
        assert!(payload == message && payload.receiver == relayer_on_b.raw_data().to_vec());
        info!("🍻 successfully find recv_packet cell on chain_b: {payload}");

        // 4. trigger WriteAck event on ChainB
        info!("send write_ack transaction to chain_b");
        let write_ack_tx = generate_write_ack_transaction(
            rt,
            chain_b_config,
            chain_b_url,
            chain_b_signer,
            recv_packet,
        )?;
        let hash = send_transaction(chain_b_url, write_ack_tx)?;
        info!(
            "🍻 successfully sent write_ack transaction to chain_b, hash = {}",
            hex::encode(hash)
        );

        // 5. listen AckPacket event on ChainA
        info!("wait ack_packet being found on chain_a");
        let ack_packet = listen_and_wait_packet_cell(rt, chain_a_url, chain_a_config, |packet| {
            packet.is_ack_packet()
        })?;
        info!("🍻 successfully find ack_packet cell on chain_a");

        // 6. comsune AckPacket cell on ChainA
        info!("send ack_packet consume transaction to chain_a");
        let consume_ack_packet_tx = generate_consume_ack_packet_transaction(
            rt,
            chain_a_config,
            chain_a_url,
            chain_a_signer,
            ack_packet,
        )?;
        let hash = send_transaction(chain_a_url, consume_ack_packet_tx)?;
        info!(
            "🍻 successfully consumed ack_packet on chain_a, hash = {}",
            hex::encode(hash)
        );

        Ok(())
    }
}

impl TestOverrides for CKB4IbcPacketTest {}

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
            &chains.handle_a().id(),
            channels.channel_id_a.value(),
        )?;
        let (chain_b_config, chain_b_url, chain_b_signer) = prepare_artificials(
            &relayer.config,
            &chains.handle_b().id(),
            channels.channel_id_b.value(),
        )?;
        info!(
            "relayer wallet balance: {} CKB on chain_a, {} CKB on chain_b",
            wallet_balance(&rt, &chain_a_url, &chain_a_config.module_lock_script())?,
            wallet_balance(&rt, &chain_b_url, &chain_b_config.module_lock_script())?
        );

        // run for first time
        println!("\n================ FIRST time run packet communication ===================\n");
        self.run_instance(
            &rt,
            &chain_a_config,
            &chain_a_url,
            &chain_a_signer,
            &chain_b_config,
            &chain_b_url,
            &chain_b_signer,
        )?;

        // FIXME: this part would block forever with no idea
        //
        // run for second time to test using useless WriteAck packet
        // println!("\n================ SECOND time run packet communication ===================\n");
        // self.run_instance(
        //     &rt,
        //     &chain_a_config,
        //     &chain_a_url,
        //     &chain_a_signer,
        //     &chain_b_config,
        //     &chain_b_url,
        //     &chain_b_signer,
        // )?;

        println!("\n================ Close Channel ===================\n");
        let channel_close_tx = generate_channel_close_init_transaction(
            &rt,
            &chain_b_config,
            &chain_b_url,
            &chain_b_signer,
        )?;
        let hash = send_transaction(&chain_b_url, channel_close_tx)?;
        info!(
            "🍻 successfully sent channel_close transaction to chain_b, hash = {}",
            hex::encode(hash)
        );
        listen_and_wait_closed_channel_cell(&rt, &chain_a_url, &chain_a_config)?;
        info!("🍻 successfully found the closed channel on chain_a");

        Ok(())
    }
}
