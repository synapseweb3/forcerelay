use ibc_relayer_types::events::IbcEvent;
use tracing::{error, info, warn};

use crate::chain::handle::ChainHandle;
use crate::chain::requests::{PageRequest, QueryClientStatesRequest};
use crate::chain::tracking::{NonCosmosTrackingId, TrackedMsgs, TrackingId};
use crate::client_state::IdentifiedAnyClientState;
use crate::config::ChainConfig;
use crate::error::ErrorDetail::LightClientVerification;
use crate::event::monitor::EventBatch;
use tendermint_light_client::errors::ErrorDetail::MissingLastBlockId;

const MAX_HEADERS_IN_BATCH: u64 = 10;
const MAX_RETRY_NUMBER: u8 = 5;
pub fn handle_event_batch<ChainA: ChainHandle, ChainB: ChainHandle>(
    eth_chain: &ChainA,
    ckb_chain: &ChainB,
    event_batch: &EventBatch,
) {
    let dst_chain = ckb_chain;
    let src_chain = eth_chain;
    if !matches!(src_chain.config().unwrap(), ChainConfig::Eth(_))
        || !matches!(dst_chain.config().unwrap(), ChainConfig::Ckb(_))
    {
        error!("ignore header relay while src chain is not eth or dst chain is not ckb");
        error!("src_chain: {:?}", src_chain);
        error!("dst_chain: {:?}", dst_chain);
        return;
    }

    for event in event_batch.events.iter() {
        if let IbcEvent::NewBlock(new_block) = event.event {
            let height = new_block.height;

            info!("start to relayer header up to {}", height.revision_height());
            let client_state = src_chain
                .build_client_state(height, crate::chain::client::ClientSettings::Other)
                .unwrap();

            let tracked_msgs = TrackedMsgs {
                msgs: vec![client_state.into()],
                tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_UPDATE_CLIENT),
            };
            // try sending header
            let ret = dst_chain.send_messages_and_wait_commit(tracked_msgs);
            if ret.is_ok() {
                info!("finish relay for ETH headers {}", height.revision_height());
                return;
            }
            // returned err indicates headers falling behind
            let err = ret.unwrap_err();
            let mut start_height = match err.detail() {
                LightClientVerification(e) => match &e.source {
                    MissingLastBlockId(height) => height.height.into(),
                    _ => u64::MAX,
                },
                _ => u64::MAX,
            };
            if start_height == u64::MAX {
                error!("receive unexpected error: {:?}", err);
                return;
            }

            let target_height = height.revision_height();
            let mut retry_number = 0;
            while start_height < target_height {
                if retry_number != 0 {
                    info!("{} time retry from height {}", retry_number, start_height);
                }
                let limit = std::cmp::min(MAX_HEADERS_IN_BATCH, target_height - start_height + 1);
                let request = QueryClientStatesRequest {
                    pagination: Some(PageRequest {
                        offset: start_height,
                        limit,
                        ..Default::default()
                    }),
                };
                let client_states = src_chain.query_clients(request).unwrap();
                let len = client_states.len() as u64;
                let end_height = start_height + len - 1;
                info!("get ETH headers from {} to {}", start_height, end_height);
                if len < limit {
                    warn!(
                        "can't find enough ETH headers to relay, expect {} but only fetch {}",
                        limit, len
                    );
                }
                let ret = send_messages(dst_chain, client_states);
                match ret {
                    Ok(_) => {
                        info!(
                            "ETH headers from {} to {} are relayed to CKB",
                            start_height, end_height
                        );
                        retry_number = 0;
                        start_height = end_height + 1;
                    }
                    Err(e) => {
                        error!("encounter error when relaying ETH header: {:?}", e);
                        retry_number += 1;
                    }
                }
                if retry_number >= MAX_RETRY_NUMBER {
                    error!(
                        "retry number {} exceeds maximum value {}. stop header sync.",
                        retry_number, MAX_RETRY_NUMBER
                    );
                    return;
                }
            }
        }
    }
}

fn send_messages<Chain: ChainHandle>(
    chain: &Chain,
    client_states: Vec<IdentifiedAnyClientState>,
) -> Result<Vec<crate::event::IbcEventWithHeight>, crate::error::Error> {
    let tracked_msgs = TrackedMsgs {
        msgs: client_states
            .into_iter()
            .map(|s| s.client_state.into())
            .collect(),
        tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_UPDATE_CLIENT),
    };
    return chain.send_messages_and_wait_commit(tracked_msgs);
}
