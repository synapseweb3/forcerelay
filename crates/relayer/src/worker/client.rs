use core::convert::Infallible;
use core::time::Duration;
use crossbeam_channel::Receiver;

use std::time::Instant;
use tendermint_light_client::errors::ErrorDetail::MissingLastBlockId;
use tracing::{debug, span, trace, warn};
use uuid::Uuid;

use ibc_relayer_types::events::IbcEvent;
use retry::delay::Fibonacci;
use retry::retry_with_index;

use crate::chain::requests::{PageRequest, QueryClientStatesRequest};
use crate::chain::tracking::{TrackedMsgs, TrackingId};
use crate::error::ErrorDetail::LightClientVerification;
use crate::util::retry::clamp_total;
use crate::util::task::{spawn_background_task, Next, TaskError, TaskHandle};
use crate::{
    chain::handle::ChainHandle,
    foreign_client::{ForeignClient, MisbehaviourResults},
};

use super::WorkerCmd;

const REFRESH_INTERVAL_SECONDS: u64 = 2;
const INITIAL_FIBONACCI_VALUE: u64 = 1;
const MAX_REFRESH_DELAY_SECONDS: u64 = 60 * 60; // 1 hour
const MAX_REFRESH_TOTAL_DELAY_SECONDS: u64 = 60 * 60 * 24; // 1 day

pub fn spawn_refresh_client<ChainA: ChainHandle, ChainB: ChainHandle>(
    mut client: ForeignClient<ChainA, ChainB>,
) -> Option<TaskHandle> {
    if client.is_expired_or_frozen() {
        warn!(
            client = %client.id,
            "skipping refresh client task on frozen client",
        );
        None
    } else {
        // Compute the refresh interval as a fraction of the client's trusting period
        // If the trusting period or the client state is not retrieved, fallback to a default value.
        let mut next_refresh = Instant::now() + Duration::from_secs(REFRESH_INTERVAL_SECONDS);
        Some(spawn_background_task(
            span!(
                tracing::Level::ERROR,
                "worker.client.refresh",
                client = %client.id,
                src_chain = %client.src_chain.id(),
                dst_chain = %client.dst_chain.id(),
            ),
            Some(Duration::from_secs(1)),
            move || {
                // This is used for integration tests until `spawn_background_task`
                // uses async instead of threads
                if Instant::now() < next_refresh {
                    return Ok(Next::Continue);
                }

                // Use retry mechanism only if `client.refresh()` fails.
                let res = retry_with_index(
                    clamp_total(
                        Fibonacci::from(Duration::from_secs(INITIAL_FIBONACCI_VALUE)),
                        Duration::from_secs(MAX_REFRESH_DELAY_SECONDS),
                        Duration::from_secs(MAX_REFRESH_TOTAL_DELAY_SECONDS),
                    ),
                    |_| client.refresh(),
                );

                match res {
                    // If `client.refresh()` was successful, update the `next_refresh` call.
                    Ok(_) => {
                        next_refresh =
                            Instant::now() + Duration::from_secs(REFRESH_INTERVAL_SECONDS);
                        Ok(Next::Continue)
                    }
                    // If `client.refresh()` failed and the retry mechanism
                    // exceeded the maximum delay, return a fatal error.
                    Err(e) => Err(TaskError::Fatal(e)),
                }
            },
        ))
    }
}

pub fn detect_misbehavior_task<ChainA: ChainHandle, ChainB: ChainHandle>(
    receiver: Receiver<WorkerCmd>,
    client: ForeignClient<ChainB, ChainA>,
) -> Option<TaskHandle> {
    if client.is_expired_or_frozen() {
        warn!(
            client = %client.id(),
            "skipping detect misbehavior task on frozen client",
        );
        return None;
    }

    let mut first_check_done = false;

    let handle = spawn_background_task(
        span!(
            tracing::Level::ERROR,
            "worker.client.misbehaviour",
            client = %client.id,
            src_chain = %client.src_chain.id(),
            dst_chain = %client.dst_chain.id(),
        ),
        Some(Duration::from_millis(600)),
        move || -> Result<Next, TaskError<Infallible>> {
            if !first_check_done {
                first_check_done = true;
                debug!("doing first check");
                let misbehavior_result = client.detect_misbehaviour_and_submit_evidence(None);
                trace!("detect misbehavior result: {:?}", misbehavior_result);
            }

            if let Ok(cmd) = receiver.try_recv() {
                match cmd {
                    WorkerCmd::IbcEvents { batch } => {
                        trace!("received batch: {:?}", batch);

                        for event_with_height in batch.events {
                            if let IbcEvent::UpdateClient(ref update) = event_with_height.event {
                                debug!("checking misbehavior for updated client");
                                let misbehavior_result =
                                    client.detect_misbehaviour_and_submit_evidence(Some(update));
                                trace!("detect misbehavior result: {:?}", misbehavior_result);

                                match misbehavior_result {
                                    MisbehaviourResults::ValidClient => {}
                                    MisbehaviourResults::VerificationError => {
                                        // can retry in next call
                                    }
                                    MisbehaviourResults::EvidenceSubmitted(_) => {
                                        // if evidence was submitted successfully then exit
                                        return Ok(Next::Abort);
                                    }
                                    MisbehaviourResults::CannotExecute => {
                                        // skip misbehaviour checking if chain does not have support for it (i.e. client
                                        // update event does not include the header)
                                        return Ok(Next::Abort);
                                    }
                                }
                            }
                        }
                    }

                    WorkerCmd::NewBlock { height, .. } => {
                        let dst_chain = client.dst_chain();
                        let src_chain = client.src_chain();
                        let client_state = src_chain
                            .build_client_state(height, crate::chain::client::ClientSettings::Eth)
                            .unwrap();

                        let tracked_msgs = TrackedMsgs {
                            msgs: vec![client_state.into()],
                            tracking_id: TrackingId::Uuid(Uuid::default()),
                        };
                        // try sending header
                        let ret = dst_chain.send_messages_and_wait_commit(tracked_msgs);
                        if ret.is_ok() {
                            trace!("finish relay for ETH headers {}", height.revision_height(),);
                            return Ok(Next::Continue);
                        }

                        let end_height = height.revision_height();
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
                            panic!("receive unexpected error: {:?}", err)
                        }
                        let mut i = start_height;
                        const LIMIT: u64 = 10;
                        // header chasing
                        while i <= end_height {
                            let limit = if LIMIT < end_height - i + 1 {
                                LIMIT
                            } else {
                                end_height - i + 1
                            };
                            let request = QueryClientStatesRequest {
                                pagination: Some(PageRequest {
                                    offset: i,
                                    limit,
                                    ..Default::default()
                                }),
                            };
                            let client_states = src_chain.query_clients(request).unwrap();
                            let len = client_states.len() as u64;
                            trace!("get ETH headers from {} to {}", i, i + len - 1);
                            let tracked_msgs = TrackedMsgs {
                                msgs: client_states
                                    .into_iter()
                                    .map(|s| s.client_state.into())
                                    .collect(),
                                tracking_id: TrackingId::Uuid(Uuid::default()),
                            };
                            let ret = dst_chain.send_messages_and_wait_commit(tracked_msgs);
                            if ret.is_ok() {
                                trace!(
                                    "ETH headers from {} to {} are relayed to CKB",
                                    i,
                                    i + len - 1
                                );
                                if len < limit {
                                    trace!("can't find enought ETH header to relay");
                                    return Ok(Next::Continue);
                                }
                                i += len;
                            } else {
                                trace!(
                                    "encounter error when sending messages: {:?}",
                                    ret.unwrap_err()
                                );
                                return Ok(Next::Continue);
                                // TODO: how to handle situation when failling to send header?
                            }
                        }
                        trace!(
                            "finish relay for ETH headers from {} to {}",
                            start_height,
                            end_height
                        );
                    }
                    WorkerCmd::ClearPendingPackets => {}
                }
            }

            Ok(Next::Continue)
        },
    );

    Some(handle)
}
