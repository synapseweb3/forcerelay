use ckb_chain_spec::consensus::TYPE_ID_CODE_HASH;
use ckb_jsonrpc_types::OutputsValidator;
use ckb_jsonrpc_types::TransactionView as JsonTxView;
use ckb_sdk::CkbRpcClient;
use ckb_sdk::{Address, AddressPayload, NetworkType};
use ckb_types::core::{ScriptHashType, TransactionView};
use ckb_types::packed::Script;
use ckb_types::prelude::{Builder, Entity, Pack};
use ckb_types::H256;
use eyre::{eyre, Result as EyreResult};
use forcerelay_ckb_sdk::ckb_ics_axon::handler::{IbcPacket, PacketStatus};
use forcerelay_ckb_sdk::ckb_ics_axon::object::Packet;
use forcerelay_ckb_sdk::ckb_rpc_client::CkbRpcClient as SdkRpcClient;
use forcerelay_ckb_sdk::config::{AddressOrScript, AddressString, Config as SdkConfig};
use forcerelay_ckb_sdk::search::{
    get_axon_metadata_cell_dep, get_channel_contract_cell_dep, IbcChannelCell,
};
use forcerelay_ckb_sdk::transaction::assemble_send_packet_partial_transaction;
use ibc_test_framework::prelude::*;
use log::info;
use relayer::config::ChainConfig;
use relayer::keyring::{KeyRing, Secp256k1KeyPair};
use std::str::FromStr;
use tokio::runtime::Runtime;

/// CKB only allow h256 as portId
fn transfer_port_id() -> PortId {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(b"transfer");
    PortId::from_str(H256::from(buf).to_string().as_str()).unwrap()
}

fn error_cast<T: ToString>(error: T) -> eyre::Error {
    eyre!("{}", error.to_string())
}

fn generate_ckb_sdk_config(
    config: &Config,
    chain: &impl ChainHandle,
    channel_id: &ChannelId,
) -> EyreResult<(SdkConfig, String)> {
    let chain_id = chain.id();
    let generic_config = config
        .find_chain(&chain_id)
        .ok_or(eyre!("{chain_id} not found"))?;
    let ckb4ibc_config = if let ChainConfig::Ckb4Ibc(config) = generic_config {
        config
    } else {
        return Err(eyre!("{chain_id} is not ckb4ibc type"));
    };
    let relayer_address = {
        let key_base: KeyRing<Secp256k1KeyPair> =
            KeyRing::new(Default::default(), "ckb", &chain_id)?;
        let relayer_pubkey = key_base.get_key(&ckb4ibc_config.key_name)?.public_key;
        let payload = AddressPayload::from_pubkey(&relayer_pubkey);
        Address::new(NetworkType::Testnet, payload, true)
    };
    let metadata_type_args = ckb4ibc_config.lc_client_type_args(5u64.try_into()?)?;
    let metadata_script = Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(metadata_type_args.to_vec().pack())
        .build();
    let channel_number = u16::from_str(
        channel_id
            .to_string()
            .split('-')
            .last()
            .expect("channel id"),
    )?;
    let sdk_config = SdkConfig {
        user_lock_script: AddressOrScript::Address(AddressString(relayer_address)),
        axon_metadata_type_script: AddressOrScript::Script(metadata_script.into()),
        channel_contract_type_id_args: ckb4ibc_config.channel_type_args.clone(),
        packet_contract_type_id_args: ckb4ibc_config.packet_type_args.clone(),
        channel_id: channel_number,
        confirmations: 1,
    };
    Ok((sdk_config, ckb4ibc_config.ckb_rpc.to_string()))
}

fn generate_send_packet_transaction(
    rt: &Runtime,
    sdk_config: &SdkConfig,
    ckb_rpc: &SdkRpcClient,
    target_port_id: String,
    target_channel_id: String,
    payload: Vec<u8>,
) -> EyreResult<TransactionView> {
    let metadata_celldep = rt
        .block_on(get_axon_metadata_cell_dep(ckb_rpc, sdk_config))
        .map_err(error_cast)?;
    let channel_contract_celldep = rt
        .block_on(get_channel_contract_cell_dep(ckb_rpc, sdk_config))
        .map_err(error_cast)?;
    let ibc_channel = rt
        .block_on(IbcChannelCell::get_latest(ckb_rpc, sdk_config))
        .map_err(error_cast)?;
    let send_packet = IbcPacket {
        packet: Packet {
            destination_port_id: target_port_id,
            destination_channel_id: target_channel_id,
            data: payload,
            ..Default::default()
        },
        tx_hash: None,
        status: PacketStatus::Send,
    };
    let (send_packet_tx, _) = assemble_send_packet_partial_transaction(
        metadata_celldep,
        channel_contract_celldep,
        sdk_config,
        ibc_channel,
        send_packet,
    )
    .map_err(error_cast)?;
    Ok(send_packet_tx.build())
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
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "send sUDT packets over the channel ({}: {}/{}, {}: {}/{})",
            chains.chain_id_a(),
            channels.port_a,
            channels.channel_id_a,
            chains.chain_id_b(),
            channels.port_b,
            channels.channel_id_b,
        );
        let rt = Runtime::new()?;

        // trigger SendPacket event on ChainA
        let (chain_a_config, chain_a_url) = generate_ckb_sdk_config(
            &relayer.config,
            chains.handle_a(),
            channels.channel_id_a.value(),
        )?;
        let chain_a_rpc = SdkRpcClient::new(chain_a_url.clone());
        let message = b"hello world".to_vec();
        let send_packet_tx = generate_send_packet_transaction(
            &rt,
            &chain_a_config,
            &chain_a_rpc,
            channels.port_b.to_string(),
            channels.channel_id_b.to_string(),
            message,
        )?;
        let response = CkbRpcClient::new(&chain_a_url)
            .send_transaction(
                send_packet_tx.data().into(),
                Some(OutputsValidator::Passthrough),
            )
            .map_err(|e| {
                eyre!(
                    "{}\n\ntransaction info: {}\n",
                    e.to_string(),
                    serde_json::to_string_pretty(&JsonTxView::from(send_packet_tx)).unwrap()
                )
            })?;
        info!(
            "successfully sent send_packet transaction to chain_a {}, hash = {}",
            chains.chain_id_a(),
            hex::encode(response)
        );

        // TODO: fetch something from ChainB that indicates ChainB processed this SendPacket event
        Ok(())
    }
}
