use ckb_ics_axon::object::State;
use ckb_jsonrpc_types::{OutputsValidator, TransactionView as JsonTxView};
use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ckb_sdk::rpc::ckb_indexer::ScriptSearchMode;
use ckb_sdk::rpc::ckb_light_client::{Order, ScriptType, SearchKey};
use ckb_sdk::traits::{CellQueryOptions, SecpCkbRawKeySigner, ValueRangeOption};
use ckb_sdk::unlock::{ScriptSigner, SecpSighashScriptSigner};
use ckb_sdk::{CkbRpcClient, HumanCapacity};
use ckb_sdk::{ScriptGroup, ScriptGroupType};
use ckb_types::core::{Capacity, ScriptHashType, TransactionBuilder, TransactionView};
use ckb_types::packed::{CellDep, CellInput, CellOutput, Script};
use ckb_types::prelude::{Builder, Entity, Pack, Unpack};
use ckb_types::H256;
use eyre::{eyre, Result as EyreResult};
use forcerelay_ckb_sdk::ckb_ics_axon::message::Envelope;
use forcerelay_ckb_sdk::ckb_rpc_client::CkbRpcClient as SdkRpcClient;
use forcerelay_ckb_sdk::config::{AddressOrScript, Config as SdkConfig};
use forcerelay_ckb_sdk::search::{
    get_axon_metadata_cell_dep, get_channel_contract_cell_dep, get_packet_contract_cell_dep,
    IbcChannelCell, PacketCell,
};
use forcerelay_ckb_sdk::transaction::{
    add_ibc_envelope, assemble_channel_close_init_partial_transaction,
    assemble_consume_ack_packet_partial_transaction, assemble_send_packet_partial_transaction,
    assemble_write_ack_partial_transaction,
};
use futures::{pin_mut, StreamExt, TryStreamExt};
use ibc_test_framework::prelude::*;
use relayer::chain::ckb::prelude::{CkbReader, TxCompleter};
use relayer::chain::ckb::rpc_client::RpcClient;
use relayer::config::ChainConfig;
use rlp::Encodable;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tiny_keccak::Hasher;
use tokio::runtime::Runtime;

use crate::consts::{AXON_IBC_HANDLER_ADDRESS, CLIENT_TYPE_ARGS, SUDT_CODE_HASH, SUDT_TYPE_ARGS};
use crate::generator::{get_lock_script, PRIVKEY};

#[derive(Serialize, Deserialize, PartialEq)]
pub struct ICS20Transfer {
    pub denom: String,
    pub amount: u64,
    pub sender: Vec<u8>,
    pub receiver: Vec<u8>,
}

impl std::fmt::Display for ICS20Transfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!(
            "Ics20_transfer {{ denom: {}, amount: {}, sender: {}, receiver: {} }}",
            self.denom,
            self.amount,
            hex::encode(&self.sender),
            hex::encode(&self.receiver)
        ))
    }
}

fn error_cast<T: ToString>(error: T) -> eyre::Error {
    eyre!("{}", error.to_string())
}

fn prepare_celldeps_and_channel(
    rt: &Runtime,
    ckb_url: &str,
    sdk_config: &SdkConfig,
) -> EyreResult<(CellDep, CellDep, CellDep, IbcChannelCell)> {
    let sdk_rpc = SdkRpcClient::new(ckb_url.to_owned());
    let metadata_celldep = rt
        .block_on(get_axon_metadata_cell_dep(&sdk_rpc, sdk_config))
        .map_err(error_cast)?;
    let channel_contract_celldep = rt
        .block_on(get_channel_contract_cell_dep(&sdk_rpc, sdk_config))
        .map_err(error_cast)?;
    let packet_contract_celldep = rt
        .block_on(get_packet_contract_cell_dep(&sdk_rpc, sdk_config))
        .map_err(error_cast)?;
    let ibc_channel = rt
        .block_on(IbcChannelCell::get_latest(&sdk_rpc, sdk_config))
        .map_err(error_cast)?;
    Ok((
        metadata_celldep,
        channel_contract_celldep,
        packet_contract_celldep,
        ibc_channel,
    ))
}

fn collect_inputs_capacity(rt: &Runtime, ckb_url: &str, tx: &TransactionView) -> EyreResult<u64> {
    let rpc_client = RpcClient::new(&ckb_url.parse().unwrap(), &ckb_url.parse().unwrap());
    let mut inputs_capacity = 0u64;
    for input in tx.input_pts_iter() {
        let input_tx = rt
            .block_on(rpc_client.get_transaction(&input.tx_hash().unpack()))
            .map_err(error_cast)?
            .unwrap()
            .transaction
            .unwrap();
        let tx = match input_tx.inner {
            ckb_jsonrpc_types::Either::Left(tx) => tx,
            ckb_jsonrpc_types::Either::Right(bytes) => {
                serde_json::from_slice(bytes.as_bytes()).unwrap()
            }
        };
        let index: u32 = input.index().unpack();
        let capacity: u64 = tx.inner.outputs[index as usize].capacity.into();
        inputs_capacity += capacity;
    }
    Ok(inputs_capacity)
}

fn fill_transaction_with_secp256k1_change(
    rt: &Runtime,
    ckb_url: &str,
    tx: TransactionView,
) -> EyreResult<TransactionView> {
    let inputs_capacity = collect_inputs_capacity(rt, ckb_url, &tx)?;
    let rpc_client = RpcClient::new(&ckb_url.parse().unwrap(), &ckb_url.parse().unwrap());
    let (_, _, address) = get_lock_script(PRIVKEY);
    let (tx, _) = rt
        .block_on(rpc_client.complete_tx_with_secp256k1_change(tx, &address, inputs_capacity, 3000))
        .map_err(error_cast)?;
    Ok(tx)
}

fn complete_partial_transaction(
    rt: &Runtime,
    partial_tx: TransactionBuilder,
    ckb_url: &str,
    sdk_config: &SdkConfig,
    envelope: Option<Envelope>,
    signer: &SecpSighashScriptSigner,
    signature_start: Option<usize>,
) -> EyreResult<TransactionView> {
    let partial_tx = partial_tx.build();
    let signature_start = signature_start.unwrap_or(partial_tx.inputs().len());
    let unsigned_tx_without_envelope =
        fill_transaction_with_secp256k1_change(rt, ckb_url, partial_tx)?;
    let unsigned_tx = envelope.map_or(unsigned_tx_without_envelope.clone(), |value| {
        add_ibc_envelope(unsigned_tx_without_envelope.as_advanced_builder(), &value).build()
    });
    let signature_end = unsigned_tx.inputs().len();
    let signed_tx = signer
        .sign_tx(
            &unsigned_tx,
            &ScriptGroup {
                script: sdk_config.module_lock_script(),
                group_type: ScriptGroupType::Lock,
                input_indices: (signature_start..signature_end).collect(),
                output_indices: vec![],
            },
        )
        .map_err(|e| tx_error_cast(e, unsigned_tx))?;
    Ok(signed_tx)
}

pub fn search_sudt_cells(
    rt: &Runtime,
    ckb_url: &str,
) -> EyreResult<(CellDep, CellInput, CellOutput, u128)> {
    let sdk_rpc = SdkRpcClient::new(ckb_url.to_owned());

    // search sUDT contract
    let sudt_contract_type_script = Script::new_builder()
        .code_hash(TYPE_ID_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(SUDT_TYPE_ARGS.as_bytes().pack())
        .build();
    let mut query = CellQueryOptions::new_type(sudt_contract_type_script);
    query.script_search_mode = Some(ScriptSearchMode::Exact);
    let sudt_contract_cell = rt
        .block_on(sdk_rpc.get_cells(query.into(), Order::Asc, 1.into(), None))
        .map_err(error_cast)?
        .objects
        .first()
        .cloned()
        .ok_or(eyre!("sudt contract not deployed"))?;
    let sudt_celldep = CellDep::new_builder()
        .out_point(sudt_contract_cell.out_point.into())
        .build();

    // search sUDT cell
    let (lock_script, _, address) = get_lock_script(PRIVKEY);
    let sudt_type_script = Script::new_builder()
        .code_hash(SUDT_CODE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(lock_script.calc_script_hash().as_slice().pack())
        .build();
    let mut query = CellQueryOptions::new_type(sudt_type_script);
    query.with_data = Some(true);
    query.data_len_range = Some(ValueRangeOption::new_exact(16));
    query.script_search_mode = Some(ScriptSearchMode::Exact);
    let sudt_cell = rt
        .block_on(sdk_rpc.get_cells(query.into(), Order::Asc, 1.into(), None))
        .map_err(error_cast)?
        .objects
        .first()
        .cloned()
        .ok_or(eyre!("sudt cell not found on {address}"))?;

    let sudt_input = CellInput::new_builder()
        .previous_output(sudt_cell.out_point.into())
        .build();
    let sudt_amount = u128::from_le_bytes(
        sudt_cell
            .output_data
            .unwrap()
            .as_bytes()
            .try_into()
            .unwrap(),
    );
    Ok((
        sudt_celldep,
        sudt_input,
        sudt_cell.output.into(),
        sudt_amount,
    ))
}

pub fn keccak256(slice: &[u8]) -> [u8; 32] {
    let mut hasher = tiny_keccak::Keccak::v256();
    hasher.update(slice);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    output
}

pub fn tx_error_cast<T: ToString>(error: T, tx: TransactionView) -> eyre::Error {
    eyre!(
        "{}\n\ntransaction info: {}\n",
        error.to_string(),
        serde_json::to_string_pretty(&JsonTxView::from(tx)).unwrap()
    )
}

pub fn prepare_artificials(
    config: &Config,
    chain_id: &ChainId,
    channel_id: &ChannelId,
) -> EyreResult<(SdkConfig, String, SecpSighashScriptSigner)> {
    let generic_config = config
        .find_chain(chain_id)
        .ok_or(eyre!("{chain_id} not found"))?;
    let ckb4ibc_config = if let ChainConfig::Ckb4Ibc(config) = generic_config {
        config
    } else {
        return Err(eyre!("{chain_id} is not ckb4ibc type"));
    };
    let (relayer_script, relayer_key, _) = get_lock_script(PRIVKEY);
    let metadata_script = Script::new_builder()
        .code_hash(ckb4ibc_config.client_code_hash.pack())
        .hash_type(ScriptHashType::Type.into())
        .args(CLIENT_TYPE_ARGS.as_bytes().pack())
        .build();
    let channel_number = u16::from_str(
        channel_id
            .to_string()
            .split('-')
            .last()
            .expect("channel id"),
    )?;
    let sdk_config = SdkConfig {
        module_lock_script: AddressOrScript::Script(relayer_script.into()),
        axon_metadata_type_script: AddressOrScript::Script(metadata_script.into()),
        axon_ibc_handler_address: AXON_IBC_HANDLER_ADDRESS,
        channel_contract_type_id_args: ckb4ibc_config.channel_type_args.clone(),
        packet_contract_type_id_args: ckb4ibc_config.packet_type_args.clone(),
        channel_id: channel_number,
        confirmations: 1,
    };
    let signer =
        SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![
            relayer_key,
        ])));
    Ok((sdk_config, ckb4ibc_config.ckb_rpc.to_string(), signer))
}

pub fn send_transaction(ckb_url: &str, tx: TransactionView) -> EyreResult<H256> {
    let hash = CkbRpcClient::new(ckb_url)
        .send_transaction(tx.data().into(), Some(OutputsValidator::Passthrough))
        .map_err(|e| tx_error_cast(e, tx))?;
    Ok(hash)
}

pub fn wallet_balance(
    rt: &Runtime,
    ckb_url: &String,
    script: &Script,
) -> EyreResult<HumanCapacity> {
    let search_key = SearchKey {
        script: script.clone().into(),
        script_type: ScriptType::Lock,
        filter: None,
        with_data: None,
        group_by_transaction: None,
        script_search_mode: Some(ScriptSearchMode::Exact),
    };
    let wallet_cells = rt
        .block_on(SdkRpcClient::new(ckb_url.clone()).get_cells(
            search_key,
            Order::Asc,
            1.into(),
            None,
        ))
        .map_err(error_cast)?;
    if let Some(wallet) = wallet_cells.objects.first() {
        let capacity: u64 = wallet.output.capacity.into();
        Ok(HumanCapacity::from(capacity))
    } else {
        Err(eyre!("no wallet on chain {ckb_url}"))
    }
}

pub fn generate_send_packet_transaction(
    rt: &Runtime,
    sdk_config: &SdkConfig,
    ckb_url: &str,
    signer: &SecpSighashScriptSigner,
    message: &ICS20Transfer,
) -> EyreResult<TransactionView> {
    // prepare ingredients
    let (sudt_celldep, sudt_input, sudt_output, total_amount) = search_sudt_cells(rt, ckb_url)?;
    let (metadata_celldep, channel_contract_celldep, _, ibc_channel) =
        prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;

    // assemble partial transaction
    let (send_packet_tx, envelope) = assemble_send_packet_partial_transaction(
        metadata_celldep,
        channel_contract_celldep,
        sdk_config,
        ibc_channel,
        serde_json::to_vec(message).unwrap(),
        0,
        0,
    )
    .map_err(error_cast)?;
    if message.amount as u128 > total_amount {
        return Err(eyre!(
            "sufficient token: {} > {total_amount}",
            message.amount
        ));
    }
    let sudt_amount = total_amount - message.amount as u128;
    let partial_tx = send_packet_tx
        .cell_dep(sudt_celldep)
        .input(sudt_input)
        .output(sudt_output)
        .output_data(sudt_amount.to_le_bytes().to_vec().pack());

    // complete partial transaction
    let signed_tx = complete_partial_transaction(
        rt,
        partial_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
        Some(1),
    )?;
    Ok(signed_tx)
}

pub fn listen_and_wait_packet_cell<F: Fn(&PacketCell) -> bool>(
    rt: &Runtime,
    ckb_url: &str,
    sdk_config: &SdkConfig,
    filter: F,
) -> EyreResult<PacketCell> {
    let sdk_rpc = SdkRpcClient::new(ckb_url.to_owned());
    let stream = PacketCell::subscribe(sdk_rpc, sdk_config.clone())
        .try_filter(|cell| futures::future::ready(filter(cell)));
    pin_mut!(stream);
    if let Some(packet) = rt.block_on(stream.next()) {
        Ok(packet.map_err(error_cast)?)
    } else {
        Err(eyre!("failed to listen packet cells"))
    }
}

pub fn generate_write_ack_transaction(
    rt: &Runtime,
    sdk_config: &SdkConfig,
    ckb_url: &str,
    signer: &SecpSighashScriptSigner,
    recv_packet: PacketCell,
) -> EyreResult<TransactionView> {
    // prepare ingredients
    let (sudt_celldep, _, sudt_output, total_amount) = search_sudt_cells(rt, ckb_url)?;
    let (metadata_celldep, channel_contract_celldep, packet_contract_celldep, ibc_channel) =
        prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;

    // assemble partial transaction
    let acknowledgement = keccak256(&recv_packet.packet.packet.rlp_bytes());
    let payload: ICS20Transfer = serde_json::from_slice(&recv_packet.packet.packet.data).unwrap();
    let (write_ack_tx, envelope) = assemble_write_ack_partial_transaction(
        metadata_celldep,
        channel_contract_celldep,
        packet_contract_celldep,
        sdk_config,
        ibc_channel,
        recv_packet,
        acknowledgement.to_vec(),
    )
    .map_err(error_cast)?;
    let sudt_amount = total_amount + payload.amount as u128;
    let partial_tx = write_ack_tx
        .cell_dep(sudt_celldep)
        .output(sudt_output)
        .output_data(sudt_amount.to_le_bytes().to_vec().pack());

    // complete partial transaction
    let signed_tx = complete_partial_transaction(
        rt,
        partial_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
        None,
    )?;
    Ok(signed_tx)
}

pub fn generate_consume_ack_packet_transaction(
    rt: &Runtime,
    sdk_config: &SdkConfig,
    ckb_url: &str,
    signer: &SecpSighashScriptSigner,
    ack_packet: PacketCell,
) -> EyreResult<TransactionView> {
    // prepare ingredients
    let (_, _, packet_contract_celldep, _) = prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;
    let (mut consume_ack_packet_tx, envelope) =
        assemble_consume_ack_packet_partial_transaction(packet_contract_celldep, ack_packet)
            .map_err(error_cast)?;
    // add output payback cell mannually
    let tx = consume_ack_packet_tx.build();
    let inputs_capacity = collect_inputs_capacity(rt, ckb_url, &tx)?;
    let (lock, _, _) = get_lock_script(PRIVKEY);
    let payback_cell = CellOutput::new_builder()
        .lock(lock)
        .capacity(Capacity::shannons(inputs_capacity).pack())
        .build();
    consume_ack_packet_tx = tx
        .as_advanced_builder()
        .output(payback_cell)
        .output_data(Default::default());
    // complete partial transaction
    let signed_tx = complete_partial_transaction(
        rt,
        consume_ack_packet_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
        None,
    )?;
    Ok(signed_tx)
}

pub fn generate_channel_close_init_transaction(
    rt: &Runtime,
    sdk_config: &SdkConfig,
    ckb_url: &str,
    signer: &SecpSighashScriptSigner,
) -> EyreResult<TransactionView> {
    let (metadata_celldep, channel_celldep, _, ibc_channel) =
        prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;
    let (channel_close_init_tx, envelope) = assemble_channel_close_init_partial_transaction(
        metadata_celldep,
        channel_celldep,
        ibc_channel,
    )
    .map_err(error_cast)?;

    let signed_tx = complete_partial_transaction(
        rt,
        channel_close_init_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
        None,
    )?;
    Ok(signed_tx)
}

pub fn listen_and_wait_closed_channel_cell(
    rt: &Runtime,
    ckb_url: &str,
    sdk_config: &SdkConfig,
) -> EyreResult<()> {
    let sdk_rpc = SdkRpcClient::new(ckb_url.to_owned());
    loop {
        let search_key = SearchKey {
            script: sdk_config.channel_cell_lock_script(false).into(),
            script_type: ScriptType::Lock,
            script_search_mode: Some(ScriptSearchMode::Exact),
            filter: None,
            with_data: Some(false),
            group_by_transaction: None,
        };
        let cells = rt
            .block_on(sdk_rpc.get_cells(search_key, Order::Desc, 1.into(), None))
            .map_err(error_cast)?;
        if cells.objects.is_empty() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let channel_cell = rt
            .block_on(IbcChannelCell::get_latest_with_open(
                &sdk_rpc, sdk_config, false,
            ))
            .map_err(error_cast)?;
        assert!(channel_cell.channel.state == State::Closed);
        return Ok(());
    }
}
