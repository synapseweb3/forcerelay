use ibc_relayer_types::events::IbcEvent;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::chain::client::ClientSettings;
use crate::chain::handle::ChainHandle;
use crate::chain::requests::{PageRequest, QueryClientStatesRequest};
use crate::chain::tracking::{NonCosmosTrackingId, TrackedMsgs, TrackingId};
use crate::client_state::IdentifiedAnyClientState;
use crate::config::ChainConfig;
use crate::error::{Error, ErrorDetail::LightClientVerification};
use crate::event::monitor::EventBatch;
use tendermint_light_client::errors::ErrorDetail::MissingLastBlockId;

const MAX_HEADERS_IN_BATCH: u64 = 128;
const MAX_SLEEP_SECONDS: u64 = 5;
const MAX_RETRY_NUMBER: u8 = 5;

pub fn handle_event_batch(
    eth_chain: &impl ChainHandle,
    ckb_chain: &impl ChainHandle,
    event_batch: &EventBatch,
) {
    if !matches!(eth_chain.config().unwrap(), ChainConfig::Eth(_))
        || !matches!(ckb_chain.config().unwrap(), ChainConfig::Ckb(_))
    {
        error!("ignore header relay while src chain is not eth or dst chain is not ckb");
        error!("src_chain: {eth_chain:?}");
        error!("dst_chain: {ckb_chain:?}");
        return;
    }

    if event_batch.events.is_empty() {
        warn!("CAUTION: start relaying EMPTY headers");
        return;
    }

    match event_batch.events[0].event {
        IbcEvent::CreateClient(_) => {
            let request = QueryClientStatesRequest { pagination: None };
            if ckb_chain
                .query_clients(request)
                .expect("ckb query_clients")
                .is_empty()
            {
                create_ethereum_light_client(eth_chain, ckb_chain, event_batch);
            }
        }
        IbcEvent::NewBlock(_) => update_ethereum_headers(eth_chain, ckb_chain, event_batch),
        _ => warn!("receiving unrecognized event"),
    }
}

fn create_ethereum_light_client(
    src_chain: &impl ChainHandle,
    dst_chain: &impl ChainHandle,
    event_batch: &EventBatch,
) {
    let checkpoint_slot = event_batch.height;
    let client_state = {
        let client_state = src_chain.build_client_state(checkpoint_slot, ClientSettings::Other);
        match client_state {
            Ok(value) => value,
            Err(err) => {
                error!("src_chain.build_client_state: {err}");
                return;
            }
        }
    };

    let tracked_msgs = TrackedMsgs {
        msgs: vec![client_state.into()],
        tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_CREATE_CLIENT),
    };

    match dst_chain.send_messages_and_wait_commit(tracked_msgs) {
        Ok(_) => info!(
            "finish creating light-client at slot {}",
            checkpoint_slot.revision_height()
        ),
        Err(error) => {
            if let Some(slot) = extract_missing_slot_from_error(&error) {
                info!("light-client already created at slot {slot}");
            }
        }
    }
}

fn update_ethereum_headers(
    src_chain: &impl ChainHandle,
    dst_chain: &impl ChainHandle,
    event_batch: &EventBatch,
) {
    // assemble client states which are transformed from finality headers
    let mut start_slot = 0;
    let target_slot = event_batch.height.revision_height();
    let msgs = event_batch
        .events
        .iter()
        .filter_map(|event| {
            if let IbcEvent::NewBlock(new_block) = event.event {
                if start_slot == 0 {
                    start_slot = new_block.height.revision_height();
                }
                let client_state = {
                    let client_state =
                        src_chain.build_client_state(new_block.height, ClientSettings::Other);
                    match client_state {
                        Ok(value) => value,
                        Err(err) => {
                            error!("src_chain.build_client_state: {err}");
                            return None;
                        }
                    }
                };
                return Some(client_state.into());
            }
            None
        })
        .collect();

    let tracked_msgs = TrackedMsgs {
        msgs,
        tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_UPDATE_CLIENT),
    };

    // try sending headers
    let result = dst_chain.send_messages_and_wait_commit(tracked_msgs);
    if result.is_ok() {
        info!("finish relaying headers [{start_slot}, {target_slot}]");
        return;
    }

    // returned err indicates headers falling behind
    start_slot = match extract_missing_slot_from_error(&result.unwrap_err()) {
        Some(slot) => {
            if slot >= target_slot {
                info!("finish relaying headers [{start_slot}, {target_slot}]");
                return;
            }
            warn!("upcoming header {start_slot} not match native tip header {slot}, start chasing");
            slot
        }
        None => return,
    };

    // chasing lost headers
    let mut retry = 0;
    while start_slot < target_slot {
        if retry > 0 {
            debug!("{retry} time retry for [{start_slot}, {target_slot}] after sleeping {MAX_SLEEP_SECONDS}s");
            std::thread::sleep(Duration::from_secs(MAX_SLEEP_SECONDS));
        }
        let limit = std::cmp::min(MAX_HEADERS_IN_BATCH, target_slot - start_slot + 1);
        let request = QueryClientStatesRequest {
            pagination: Some(PageRequest {
                offset: start_slot,
                limit,
                ..Default::default()
            }),
        };
        let client_states = match src_chain.query_clients(request) {
            Ok(value) => value,
            Err(err) => {
                error!("src_chain.query_clients: {err}, skip this try");
                return;
            }
        };
        let end_slot = start_slot + client_states.len() as u64 - 1;
        info!("send chasing headers [{start_slot}, {end_slot}]");
        match send_messages(dst_chain, client_states) {
            Ok(_) => {
                let mut retry_log =
                    format!("headers [{start_slot}, {end_slot}] are relayed to ckb, ");
                if end_slot < target_slot - 1 {
                    retry_log += &format!("keep chasing to {target_slot}");
                } else {
                    retry_log += "chasing complete";
                }
                debug!("{retry_log}");
                retry = 0;
                start_slot = end_slot + 1;
            }
            Err(error) => {
                if let Some(slot) = extract_missing_slot_from_error(&error) {
                    debug!("adjust start_slot and continue retry: {error}");
                    start_slot = slot;
                } else {
                    retry += 1;
                }
            }
        }
        if retry >= MAX_RETRY_NUMBER {
            error!("retry number {retry} exceeds the max {MAX_RETRY_NUMBER}, stop and listening to the next batch of headers");
            return;
        }
    }
}

fn send_messages<Chain: ChainHandle>(
    chain: &Chain,
    client_states: Vec<IdentifiedAnyClientState>,
) -> Result<Vec<crate::event::IbcEventWithHeight>, Error> {
    let tracked_msgs = TrackedMsgs {
        msgs: client_states
            .into_iter()
            .map(|s| s.client_state.into())
            .collect(),
        tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_UPDATE_CLIENT),
    };
    chain.send_messages_and_wait_commit(tracked_msgs)
}

fn extract_missing_slot_from_error(error: &Error) -> Option<u64> {
    if let LightClientVerification(verify_error) = error.detail() {
        if let MissingLastBlockId(height) = &verify_error.source {
            return Some(height.height.into());
        }
    }
    error!("unexpected error: {error}");
    None
}
