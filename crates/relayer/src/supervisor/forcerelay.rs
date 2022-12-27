use ibc_relayer_types::events::IbcEvent;
use tracing::{error, info};

use crate::chain::handle::ChainHandle;
use crate::chain::requests::{PageRequest, QueryClientStatesRequest};
use crate::chain::tracking::{NonCosmosTrackingId, TrackedMsgs, TrackingId};
use crate::config::ChainConfig;
use crate::error::ErrorDetail::LightClientVerification;
use crate::event::monitor::EventBatch;
use tendermint_light_client::errors::ErrorDetail::MissingLastBlockId;

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
        error!("{:?}", src_chain);
        error!("{:?}", dst_chain);
        error!("ignore header relay while src chain is not eth or dst chain is not ckb");
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
            let start_height = match err.detail() {
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
            let limit = height.revision_height() - start_height + 1;
            let request = QueryClientStatesRequest {
                pagination: Some(PageRequest {
                    offset: start_height,
                    limit,
                    ..Default::default()
                }),
            };
            let client_states = src_chain.query_clients(request).unwrap();
            let len = client_states.len() as u64;
            info!(
                "get ETH headers from {} to {}",
                start_height,
                start_height + len - 1
            );
            let tracked_msgs = TrackedMsgs {
                msgs: client_states
                    .into_iter()
                    .map(|s| s.client_state.into())
                    .collect(),
                tracking_id: TrackingId::Static(NonCosmosTrackingId::ETH_UPDATE_CLIENT),
            };
            let ret = dst_chain.send_messages_and_wait_commit(tracked_msgs);
            match ret {
                Ok(_) => {
                    info!(
                        "ETH headers from {} to {} are relayed to CKB",
                        start_height,
                        start_height + len - 1
                    );
                    if len < limit {
                        info!("warning: can't find enought ETH header to relay");
                    }
                }
                Err(_) => {
                    error!(
                        "encounter error when relaying ETH header: {:?}",
                        ret.unwrap_err()
                    );
                    // TODO: how to handle fails of sending header?
                }
            }
        }
    }
}
