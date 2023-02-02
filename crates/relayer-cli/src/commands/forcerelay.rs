use core::convert::Infallible;
use core::time::Duration;
use std::ops::Deref;

use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};
use tracing::error_span;

use ibc_relayer::chain::handle::{CachingChainHandle, ChainHandle};
use ibc_relayer::event::monitor::{Error as EventError, ErrorDetail as EventErrorDetail};
use ibc_relayer::registry::SharedRegistry;
use ibc_relayer::supervisor::forcerelay::handle_event_batch;
use ibc_relayer::util::task::{spawn_background_task, Next, TaskError};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;

use crate::conclude::Output;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct ForcerelayCmd {
    #[clap(
        long = "ethereum-chain-id",
        required = true,
        help = "Identifier of the Ethereum chain that hosts the client"
    )]
    eth_chain: ChainId,

    #[clap(
        long = "ckb-chain-id",
        required = true,
        help = "Identifier of the Ckb chain that hosts the client"
    )]
    ckb_chain: ChainId,
}

impl Runnable for ForcerelayCmd {
    fn run(&self) {
        let config = (*app_config()).clone();
        let registry = SharedRegistry::<CachingChainHandle>::new(config);
        let eth = registry.get_or_spawn(&self.eth_chain).unwrap_or_else(|e| {
            Output::error(format!("Forcerelay failed to start ethereum: {e}")).exit()
        });
        let ckb = registry.get_or_spawn(&self.ckb_chain).unwrap_or_else(|e| {
            Output::error(format!("Forcerelay failed to start ckb: {e}")).exit()
        });
        let eth_subscription = eth.subscribe().unwrap_or_else(|e| {
            Output::error(format!(
                "Forcerelay failed to subscribe ethereum events: {e}"
            ))
            .exit()
        });

        spawn_background_task(
            error_span!("worker.forcerelay"),
            Some(Duration::from_secs(5)),
            move || -> Result<Next, TaskError<Infallible>> {
                if let Ok(batch) = eth_subscription.try_recv() {
                    match batch.deref() {
                        Ok(batch) => {
                            handle_event_batch(&eth, &ckb, batch);
                        }
                        Err(EventError(EventErrorDetail::SubscriptionCancelled(_), _)) => {
                            warn!("event subscription was cancelled, clearing pending packets");
                        }
                        Err(e) => {
                            error!("error when receiving event batch: {e}")
                        }
                    }
                }
                Ok(Next::Continue)
            },
        )
        .join();
    }
}
