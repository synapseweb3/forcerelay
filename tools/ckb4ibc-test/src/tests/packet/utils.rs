use ckb_ics_axon::consts::PACKET_CELL_CAPACITY;
use ckb_jsonrpc_types::{OutputsValidator, TransactionView as JsonTxView};
use ckb_sdk::rpc::ckb_light_client::{Order, ScriptType, SearchKey};
use ckb_sdk::traits::SecpCkbRawKeySigner;
use ckb_sdk::unlock::{ScriptSigner, SecpSighashScriptSigner};
use ckb_sdk::{AddressPayload, CkbRpcClient, HumanCapacity};
use ckb_sdk::{ScriptGroup, ScriptGroupType};
use ckb_types::core::{ScriptHashType, TransactionBuilder, TransactionView};
use ckb_types::packed::{CellDep, CellOutput, Script};
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
    add_ibc_envelope, assemble_consume_ack_packet_partial_transaction,
    assemble_send_packet_partial_transaction, assemble_write_ack_partial_transaction,
};
use futures::{pin_mut, StreamExt, TryStreamExt};
use ibc_test_framework::prelude::*;
use relayer::chain::ckb::prelude::{CkbReader, TxCompleter};
use relayer::chain::ckb::rpc_client::RpcClient;
use relayer::config::ChainConfig;
use secp256k1::{Secp256k1, SecretKey};
use std::str::FromStr;
use tokio::runtime::Runtime;

use crate::consts::CLIENT_TYPE_ARGS;
use crate::generator::{get_lock_script, PRIVKEY};

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

fn fill_transaction_with_secp256k1_change(
    rt: &Runtime,
    ckb_url: &str,
    tx: TransactionView,
) -> EyreResult<TransactionView> {
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
) -> EyreResult<TransactionView> {
    let partial_tx = partial_tx.build();
    let signature_start = partial_tx.inputs().len();
    let unsigned_tx_without_envelope =
        fill_transaction_with_secp256k1_change(rt, ckb_url, partial_tx)?;
    let unsigned_tx = envelope.map_or(unsigned_tx_without_envelope.clone(), |value| {
        add_ibc_envelope(unsigned_tx_without_envelope.as_advanced_builder(), &value).build()
    });
    let signature_end = unsigned_tx.inputs().len();
    println!("signature_start = {signature_start}, signature_end = {signature_end}");
    let signed_tx = signer
        .sign_tx(
            &unsigned_tx,
            &ScriptGroup {
                script: sdk_config.user_lock_script(),
                group_type: ScriptGroupType::Lock,
                input_indices: (signature_start..signature_end).collect(),
                output_indices: vec![],
            },
        )
        .map_err(|e| tx_error_cast(e, unsigned_tx))?;
    Ok(signed_tx)
}

pub fn tx_error_cast<T: ToString>(error: T, tx: TransactionView) -> eyre::Error {
    eyre!(
        "{}\n\ntransaction info: {}\n",
        error.to_string(),
        serde_json::to_string_pretty(&JsonTxView::from(tx)).unwrap()
    )
}

pub fn transfer_port_id() -> PortId {
    let relayer_key = SecretKey::from_str(PRIVKEY).unwrap();
    let address = AddressPayload::from_pubkey(&relayer_key.public_key(&Secp256k1::default()));
    let script: Script = (&address).into();
    let script_hash = script.calc_script_hash();
    PortId::from_str(&hex::encode(script_hash.as_slice())).unwrap()
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
        user_lock_script: AddressOrScript::Script(relayer_script.into()),
        axon_metadata_type_script: AddressOrScript::Script(metadata_script.into()),
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
        script_search_mode: None,
        filter: None,
        with_data: None,
        group_by_transaction: None,
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
    payload: Vec<u8>,
) -> EyreResult<TransactionView> {
    // prepare ingredients
    let (metadata_celldep, channel_contract_celldep, _, ibc_channel) =
        prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;
    // assemble partial transaction
    let (send_packet_tx, envelope) = assemble_send_packet_partial_transaction(
        metadata_celldep,
        channel_contract_celldep,
        sdk_config,
        ibc_channel,
        payload,
        0,
        0,
    )
    .map_err(error_cast)?;
    // complete partial transaction
    let signed_tx = complete_partial_transaction(
        rt,
        send_packet_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
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
    acknowledgement: Vec<u8>,
) -> EyreResult<TransactionView> {
    // prepare ingredients
    let (metadata_celldep, channel_contract_celldep, packet_contract_celldep, ibc_channel) =
        prepare_celldeps_and_channel(rt, ckb_url, sdk_config)?;
    // assemble partial transaction
    let (write_ack_tx, envelope) = assemble_write_ack_partial_transaction(
        metadata_celldep,
        channel_contract_celldep,
        packet_contract_celldep,
        sdk_config,
        ibc_channel,
        recv_packet,
        acknowledgement,
    )
    .map_err(error_cast)?;
    // complete partial transaction
    let signed_tx = complete_partial_transaction(
        rt,
        write_ack_tx,
        ckb_url,
        sdk_config,
        Some(envelope),
        signer,
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
    let mut consume_ack_packet_tx =
        assemble_consume_ack_packet_partial_transaction(packet_contract_celldep, ack_packet)
            .map_err(error_cast)?;
    // add output payback cell mannually
    let (lock, _, _) = get_lock_script(PRIVKEY);
    let payback_cell = CellOutput::new_builder()
        .lock(lock)
        .capacity(PACKET_CELL_CAPACITY.pack())
        .build();
    consume_ack_packet_tx = consume_ack_packet_tx.output(payback_cell);
    // complete partial transaction
    let signed_tx =
        complete_partial_transaction(rt, consume_ack_packet_tx, ckb_url, sdk_config, None, signer)?;
    Ok(signed_tx)
}
