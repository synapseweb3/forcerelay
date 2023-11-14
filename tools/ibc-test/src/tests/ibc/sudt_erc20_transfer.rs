use std::{collections::HashMap, process::Command};

use bytes::Bytes;
use ckb_sdk::{
    constants::{SIGHASH_TYPE_HASH, TYPE_ID_CODE_HASH},
    traits::{
        CellCollector, CellQueryOptions, DefaultCellCollector, DefaultCellDepResolver,
        DefaultHeaderDepResolver, DefaultTransactionDependencyProvider, SecpCkbRawKeySigner,
        ValueRangeOption,
    },
    tx_builder::{balance_tx_capacity, unlock_tx, CapacityBalancer},
    unlock::{ScriptUnlocker, SecpSighashUnlocker},
    CkbRpcClient, ScriptId,
};
use ckb_types::{
    core::{BlockView, Capacity, ScriptHashType, TransactionBuilder, TransactionView},
    packed,
    prelude::*,
};
use forcerelay_ckb_sdk::{
    config::AddressOrScript,
    search::{get_latest_cell_by_type_script, IbcChannelCell, PacketCell},
    transaction::{
        add_ibc_envelope, assemble_consume_ack_packet_partial_transaction,
        assemble_send_packet_partial_transaction, assemble_write_ack_partial_transaction,
    },
};
use futures::TryStreamExt;
use ibc_test_framework::{
    bootstrap::binary::channel::{bootstrap_channel_with_connection, BootstrapChannelOptions},
    prelude::*,
    relayer::axon::transfer::read_deployed_contracts,
};
use prost::Message;
use tokio::runtime::Runtime;

use crate::{
    consts::CLIENT_TYPE_ARGS,
    generator::{get_lock_script, PRIVKEY},
    tests::ckb::packet::utils::search_sudt_cells,
};

/// Test CKB SUDT (source) <-> Axon ERC20 (sink).
///
/// Chain A must be ckb, and chain B must be axon.
pub struct SudtErc20TransferTest;

impl TestOverrides for SudtErc20TransferTest {}

impl BinaryConnectionTest for SudtErc20TransferTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        connection: ConnectedConnection<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let rt = Runtime::new()?;
        let ckb_config = chains.handle_a.config().unwrap();
        let ckb_config = ckb_config.ckb4ibc();
        let (ckb_sender_lock_script, ckb_sender_key, _) = get_lock_script(PRIVKEY);
        let ckb_url = ckb_config.ckb_rpc.to_string();

        // First account of ibc-solidity-contract axon network hd provider.
        let axon_receiver = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
        let axon_sender_address_binary = hex::decode(&axon_receiver[2..]).unwrap();

        log::info!("deploy ibc-sudt-transfer contract");
        let ibc_sudt_transfer = include_bytes!("../../../contracts/ibc-sudt-transfer");
        let first_input = get_capacity_input(&ckb_url, ckb_sender_lock_script.clone())?;
        let ibc_sudt_transfer_type_id_args = {
            let mut hasher = ckb_hash::new_blake2b();
            hasher.update(first_input.as_slice());
            hasher.update(&0u64.to_le_bytes());
            let mut ret = [0u8; 32];
            hasher.finalize(&mut ret);
            ret
        };
        let ibc_sudt_transfer_type_script = packed::Script::new_builder()
            .hash_type(ScriptHashType::Type.into())
            .code_hash(TYPE_ID_CODE_HASH.pack())
            .args(ibc_sudt_transfer_type_id_args[..].pack())
            .build();
        let ibc_sudt_transfer_output = packed::CellOutput::new_builder()
            .lock(packed::Script::default())
            .type_(Some(ibc_sudt_transfer_type_script.clone()).pack())
            .build_exact_capacity(Capacity::bytes(ibc_sudt_transfer.len()).unwrap())
            .unwrap();
        let tx = TransactionBuilder::default()
            .input(first_input)
            .output(ibc_sudt_transfer_output)
            .output_data(ibc_sudt_transfer.pack())
            .witness(sighash_placeholder_witness().as_bytes().pack())
            .build();
        let tx = complete_tx(
            &ckb_url,
            &tx,
            ckb_sender_lock_script.clone(),
            ckb_sender_key,
        )
        .unwrap();
        let tx_hash = send_transaction(&ckb_url, tx).unwrap();
        log::info!("deployed ibc-sudt-transfer");
        let sudt_transfer_contract_out_point = packed::OutPoint::new_builder()
            .tx_hash(tx_hash.pack())
            .index(0u32.pack())
            .build();

        // Create channel (ibc-sudt-transfer (st-lock) <-> ICS20TransferERC20)
        log::info!("create channel");
        let zero = &[0u8; 32];
        let args = Args {
            channel_contract_code_hash: zero,
            channel_id: 0,
            client_id: zero,
            packet_contract_code_hash: zero,
        }
        .encode();
        let ibc_sudt_transfer_lock = packed::Script::new_builder()
            .hash_type(ScriptHashType::Type.into())
            .code_hash(ibc_sudt_transfer_type_script.calc_script_hash())
            .args(args.pack())
            .build();
        let port_a_binary = ibc_sudt_transfer_lock.calc_script_hash().unpack().0;
        // CKB port id is hex(lock hash).
        let port_a: PortId = hex::encode(port_a_binary).parse().unwrap();
        // ICS20TransferERC20 is bound to port transfer in axon chain
        // initialization.
        let port_b: PortId = "transfer".parse().unwrap();
        let bootstrap_options = BootstrapChannelOptions::default()
            .order(Order::Unordered)
            .version("ics20".parse().unwrap())
            .bootstrap_with_random_ids(false);
        let channels = bootstrap_channel_with_connection(
            &chains.handle_a,
            &chains.handle_b,
            connection,
            &DualTagged::new(port_a).as_ref(),
            &DualTagged::new(port_b).as_ref(),
            bootstrap_options,
        )
        .unwrap();

        // Send sudt ckb -> axon
        log::info!("send ckb SUDT -> axon ERC20");
        let metadata_script = packed::Script::new_builder()
            .code_hash(ckb_config.client_code_hash.pack())
            .hash_type(ScriptHashType::Type.into())
            .args(CLIENT_TYPE_ARGS.as_bytes().pack())
            .build();
        let sdk_config = forcerelay_ckb_sdk::config::Config {
            module_lock_script: AddressOrScript::Script(ibc_sudt_transfer_lock.clone().into()),
            axon_metadata_type_script: AddressOrScript::Script(metadata_script.clone().into()),
            channel_contract_type_id_args: ckb_config.channel_type_args.clone(),
            packet_contract_type_id_args: ckb_config.packet_type_args.clone(),
            channel_id: channels
                .channel_id_a
                .to_string()
                .split('-')
                .last()
                .unwrap()
                .parse()
                .unwrap(),
            confirmations: 1,
        };
        let (sudt_cell_dep, sudt_input, sudt_output, _amount) =
            search_sudt_cells(&rt, &ckb_url).unwrap();
        let sudt_type_script = sudt_output.type_().to_opt().unwrap();

        // Create st-cell.
        log::info!("create st-cell");
        let st_cell_output = packed::CellOutput::new_builder()
            .lock(ibc_sudt_transfer_lock.clone())
            .type_(Some(sudt_type_script.clone()).pack())
            .build_exact_capacity(Capacity::bytes(16).unwrap())
            .unwrap();
        let tx = TransactionBuilder::default()
            .output(st_cell_output.clone())
            .output_data(0u128.to_le_bytes().pack())
            .cell_dep(sudt_cell_dep.clone())
            .build();
        let tx = complete_tx(
            &ckb_url,
            &tx,
            ckb_sender_lock_script.clone(),
            ckb_sender_key,
        )
        .unwrap();
        let tx_hash = send_transaction(&ckb_url, tx).unwrap();
        log::info!("deployed st-cell");
        log::info!("send transfer packet");
        let st_cell = packed::OutPoint::new_builder()
            .tx_hash(tx_hash.pack())
            .index(0u32.pack())
            .build();
        let denom = hex::encode(sudt_type_script.calc_script_hash().unpack());
        let ckb_sender_hash_20 =
            ckb_sender_lock_script.calc_script_hash().unpack().0[..20].to_vec();
        let packet_data = FungibleTokenPacketData {
            amount: 999,
            receiver: axon_sender_address_binary.clone(),
            sender: ckb_sender_hash_20.clone(),
            denom: denom.clone(),
        }
        .encode_to_vec();
        let sdk_client = forcerelay_ckb_sdk::ckb_rpc_client::CkbRpcClient::new(ckb_url.clone());
        let metadata_cell = rt
            .block_on(get_latest_cell_by_type_script(
                &sdk_client,
                metadata_script.into(),
            ))
            .unwrap();
        let channel_contract_cell = rt
            .block_on(get_latest_cell_by_type_script(
                &sdk_client,
                sdk_config.channel_contract_type_script().into(),
            ))
            .unwrap();
        let packet_contract_cell = rt
            .block_on(get_latest_cell_by_type_script(
                &sdk_client,
                sdk_config.packet_contract_type_script().into(),
            ))
            .unwrap();
        let channel = rt
            .block_on(IbcChannelCell::get_latest(&sdk_client, &sdk_config))
            .unwrap();
        let (send_packet_tx, envelope) = assemble_send_packet_partial_transaction(
            simple_dep(metadata_cell.out_point.clone().into()),
            simple_dep(channel_contract_cell.out_point.clone().into()),
            &sdk_config,
            channel,
            packet_data,
            0,
            0,
        )
        .unwrap();
        let send_packet_tx = send_packet_tx
            .input(simple_input(st_cell))
            .output(st_cell_output.clone())
            .output_data(999u128.to_le_bytes().pack())
            // Third input and third witness.
            .input(sudt_input)
            .witness(sighash_placeholder_witness().as_bytes().pack())
            .cell_dep(sudt_cell_dep.clone())
            .cell_dep(simple_dep(sudt_transfer_contract_out_point.clone()));
        let send_packet_tx = add_ibc_envelope(send_packet_tx, &envelope).build();
        let st_cell_idx = send_packet_tx.outputs().len() - 1;
        let send_packet_tx = complete_tx(
            &ckb_url,
            &send_packet_tx,
            ckb_sender_lock_script.clone(),
            ckb_sender_key,
        )
        .unwrap();
        let st_cell = packed::OutPoint::new_builder()
            .tx_hash(send_packet_tx.hash())
            .index(st_cell_idx.pack())
            .build();
        send_transaction(&ckb_url, send_packet_tx).unwrap();
        log::info!("sent transfer");

        // Check balance on axon and send back with a truffle script.
        log::info!("check ERC20 balance and send back");
        let transfer_contract_address =
            read_deployed_contracts(&chains.node_b.0.chain_driver.home_path)
                .unwrap()
                .transfer_contract_address;
        let axon_port = chains.node_b.0.chain_driver.rpc_port;
        let status = Command::new("yarn")
            .arg("truffle")
            .arg("exec")
            .arg("--network")
            .arg("axon")
            .arg("scripts/check_balance_and_send_back.js")
            .current_dir(std::env::var("IBC_CONTRACTS_SRC_PATH").unwrap())
            .env("AXON_HTTP_RPC_URL", format!("http://localhost:{axon_port}"))
            .env(
                "TRANSFER_CONTRACT_ADDRESS",
                format!("0x{}", hex::encode(transfer_contract_address)),
            )
            .env("SENDER", format!("0x{}", hex::encode(&ckb_sender_hash_20)))
            .env("CHANNEL", channels.channel_id_b.0.as_str())
            .env("DENOM", &denom)
            .status()?;
        assert!(status.success());
        log::info!("checked ERC20 balance and sent back");

        log::info!("consume ack");
        let ack_packets = PacketCell::subscribe(sdk_client.clone(), sdk_config.clone())
            .try_filter(|cell| futures::future::ready(cell.is_ack_packet()));
        tokio::pin!(ack_packets);
        let ack_packet = rt.block_on(ack_packets.try_next()).unwrap().unwrap();
        let (tx, envelope) = assemble_consume_ack_packet_partial_transaction(
            simple_dep(packet_contract_cell.out_point.clone().into()),
            ack_packet,
        )
        .unwrap();
        let tx = tx
            .input(get_capacity_input(
                &ckb_url,
                ckb_sender_lock_script.clone(),
            )?)
            .witness(sighash_placeholder_witness().as_bytes().pack())
            .input(simple_input(st_cell))
            .output(st_cell_output.clone())
            .output_data(999u128.to_le_bytes().pack())
            .cell_dep(sudt_cell_dep.clone())
            .cell_dep(simple_dep(sudt_transfer_contract_out_point.clone()));
        let tx = add_ibc_envelope(tx, &envelope).build();
        let st_cell_idx = tx.outputs().len() - 1;
        let send_packet_tx = complete_tx(
            &ckb_url,
            &tx,
            ckb_sender_lock_script.clone(),
            ckb_sender_key,
        )
        .unwrap();
        let st_cell = packed::OutPoint::new_builder()
            .tx_hash(send_packet_tx.hash())
            .index(st_cell_idx.pack())
            .build();
        send_transaction(&ckb_url, send_packet_tx).unwrap();
        log::info!("consumed ack");

        // Receive sudt on ckb (write ack with forcerelay-ckb-sdk, transfer SUDT
        // from st-lock to receiver)
        log::info!("Receive SUDT");
        let recv_packets = PacketCell::subscribe(sdk_client.clone(), sdk_config.clone())
            .try_filter(|cell| futures::future::ready(cell.is_recv_packet()));
        tokio::pin!(recv_packets);
        let recv_packet = rt.block_on(recv_packets.try_next()).unwrap().unwrap();
        let channel = rt
            .block_on(IbcChannelCell::get_latest(&sdk_client, &sdk_config))
            .unwrap();
        let (tx, envelope) = assemble_write_ack_partial_transaction(
            simple_dep(metadata_cell.out_point.into()),
            simple_dep(channel_contract_cell.out_point.into()),
            simple_dep(packet_contract_cell.out_point.into()),
            &sdk_config,
            channel,
            recv_packet,
            vec![1],
        )
        .unwrap();
        let tx = tx
            // Capacity input and witness. Third input/witness.
            .input(get_capacity_input(
                &ckb_url,
                ckb_sender_lock_script.clone(),
            )?)
            .witness(sighash_placeholder_witness().as_bytes().pack())
            // st-cell input/output
            .input(simple_input(st_cell))
            .output(st_cell_output)
            // 999 - 499 = 500.
            .output_data(500u128.to_le_bytes().pack())
            // received sudt output.
            .output(
                packed::CellOutput::new_builder()
                    .lock(ckb_sender_lock_script.clone())
                    .type_(Some(sudt_type_script.clone()).pack())
                    .build_exact_capacity(Capacity::bytes(16).unwrap())
                    .unwrap(),
            )
            .output_data(499u128.to_le_bytes().pack())
            .cell_dep(sudt_cell_dep)
            .cell_dep(simple_dep(sudt_transfer_contract_out_point.clone()));
        let tx = add_ibc_envelope(tx, &envelope).build();
        let tx = complete_tx(
            &ckb_url,
            &tx,
            ckb_sender_lock_script.clone(),
            ckb_sender_key,
        )
        .unwrap();
        let axon_events = chains.handle_b.subscribe().unwrap();
        send_transaction(&ckb_url, tx).unwrap();
        log::info!("Received SUDT");

        log::info!("check ack on axon");
        loop {
            let batch = axon_events.recv().unwrap();
            if (*batch)
                .as_ref()
                .unwrap()
                .events
                .iter()
                // Use string comparison instead of enum matching because
                // relayer_types is not a direct dep.
                .any(|e| e.event.event_type().as_str() == "acknowledge_packet")
            {
                break;
            }
        }
        log::info!("checked ack on axon");

        println!("\n================ Close Channel ===================\n");
        channels.channel.build_chan_close_init_and_send()?;

        Ok(())
    }
}

fn get_capacity_input(ckb_url: &str, lock: packed::Script) -> eyre::Result<packed::CellInput> {
    let mut query = CellQueryOptions::new_lock(lock);
    query.data_len_range = Some(ValueRangeOption::new_exact(0));
    query.secondary_script_len_range = Some(ValueRangeOption::new_exact(0));
    let (cells, _) = DefaultCellCollector::new(ckb_url).collect_live_cells(&query, false)?;
    eyre::ensure!(!cells.is_empty(), "failed to get capacity input");
    let first_input = simple_input(cells[0].out_point.clone());
    Ok(first_input)
}

fn simple_dep(o: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(o).build()
}

fn simple_input(o: packed::OutPoint) -> packed::CellInput {
    packed::CellInput::new_builder().previous_output(o).build()
}

/// Balance and sign tx with ckb sdk. Modified from ckb sdk example.
///
/// Placeholder witness for the sighash inputs should have already been added if
/// there are already sighash inputs.
fn complete_tx(
    ckb_rpc: &str,
    tx: &TransactionView,
    sender: packed::Script,
    sender_key: secp256k1::SecretKey,
) -> eyre::Result<TransactionView> {
    // Build ScriptUnlocker
    let signer = SecpCkbRawKeySigner::new_with_secret_keys(vec![sender_key]);
    let sighash_unlocker = SecpSighashUnlocker::from(Box::new(signer) as Box<_>);
    let sighash_script_id = ScriptId::new_type(SIGHASH_TYPE_HASH.clone());
    let mut unlockers = HashMap::default();
    unlockers.insert(
        sighash_script_id,
        Box::new(sighash_unlocker) as Box<dyn ScriptUnlocker>,
    );

    // Build CapacityBalancer
    let placeholder_witness = sighash_placeholder_witness();
    let balancer = CapacityBalancer::new_simple(sender, placeholder_witness, 1000);

    // Build:
    //   * CellDepResolver
    //   * HeaderDepResolver
    //   * CellCollector
    //   * TransactionDependencyProvider
    let ckb_client = CkbRpcClient::new(ckb_rpc);
    let cell_dep_resolver = {
        let genesis_block = ckb_client.get_block_by_number(0.into())?.unwrap();
        DefaultCellDepResolver::from_genesis(&BlockView::from(genesis_block))?
    };
    let header_dep_resolver = DefaultHeaderDepResolver::new(ckb_rpc);
    let mut cell_collector = DefaultCellCollector::new(ckb_rpc);
    let tx_dep_provider = DefaultTransactionDependencyProvider::new(ckb_rpc, 10);

    // Add sighash dep manually because the balancer may not add it if the tx
    // already have inputs from sender.
    let sighash = cell_dep_resolver.sighash_dep().unwrap().0.clone();
    let tx = tx.as_advanced_builder().cell_dep(sighash).build();

    let tx = balance_tx_capacity(
        &tx,
        &balancer,
        &mut cell_collector,
        &tx_dep_provider,
        &cell_dep_resolver,
        &header_dep_resolver,
    )?;

    let (tx, _) = unlock_tx(tx, &tx_dep_provider, &unlockers)?;

    Ok(tx)
}

fn sighash_placeholder_witness() -> packed::WitnessArgs {
    packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::from_static(&[0u8; 65])).pack())
        .build()
}

pub struct Args<'a> {
    pub client_id: &'a [u8; 32],
    pub channel_id: u16,
    pub channel_contract_code_hash: &'a [u8; 32],
    pub packet_contract_code_hash: &'a [u8; 32],
}

impl<'a> Args<'a> {
    pub fn encode(&self) -> Vec<u8> {
        [
            self.client_id,
            &u16::to_be_bytes(self.channel_id)[..],
            self.channel_contract_code_hash,
            self.packet_contract_code_hash,
        ]
        .concat()
    }
}

#[derive(Message)]
pub struct FungibleTokenPacketData {
    /// hex(sudt type script)
    #[prost(string, tag = "1")]
    pub denom: String,
    /// SUDT amount.
    #[prost(uint64, tag = "2")]
    pub amount: u64,
    /// For ckb address, this should be abi.encodePacked(ckb_blake2b(packed lock script)[..20])
    #[prost(bytes, tag = "3")]
    pub sender: Vec<u8>,
    /// For ckb address, this should be abi.encodePacked(ckb_blake2b(packed lock script)[..20])
    #[prost(bytes, tag = "4")]
    pub receiver: Vec<u8>,
}

fn send_transaction(url: &str, tx: TransactionView) -> eyre::Result<[u8; 32]> {
    let client = CkbRpcClient::new(url);
    let tx_hash = client.send_transaction(
        tx.data().into(),
        Some(ckb_jsonrpc_types::OutputsValidator::Passthrough),
    )?;
    log::info!("sent transaction {tx_hash}");
    loop {
        let tx = client.get_transaction_status(tx_hash.clone())?;
        match tx.tx_status.status {
            ckb_jsonrpc_types::Status::Committed => break,
            ckb_jsonrpc_types::Status::Rejected => panic!("rejected"),
            ckb_jsonrpc_types::Status::Unknown => panic!("unknown"),
            _ => {}
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    log::info!("transaction committed {tx_hash}");
    Ok(tx_hash.0)
}
