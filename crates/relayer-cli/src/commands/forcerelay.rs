use core::convert::Infallible;
use core::time::Duration;
use std::ops::Deref;
use std::panic::PanicInfo;
use std::sync::Arc;

use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};
use tracing::error_span;

use ibc_relayer::chain::handle::{CachingChainHandle, ChainHandle};
use ibc_relayer::event::monitor::{Error as EventError, ErrorDetail as EventErrorDetail};
use ibc_relayer::registry::SharedRegistry;
use ibc_relayer::supervisor::forcerelay::handle_eth_ckb_event_batch;
use ibc_relayer::config::CHAIN_CONFIG_PATH;
use ibc_relayer::util::task::{spawn_background_task, Next, TaskError, TaskHandle};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;

use crate::conclude::Output;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct EthCkbCmd {
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

impl Runnable for EthCkbCmd {
    fn run(&self) {
        let config = (*app_config()).clone();
        let config_path = app_config_path().expect("config path isn't set");
        if let Err(e) = CHAIN_CONFIG_PATH.set(config_path) {
            let path = e.into();
            let cur_path = CHAIN_CONFIG_PATH.get().unwrap();
            assert_eq!(path, cur_path, "config path is changed");
        }

        let registry = SharedRegistry::<CachingChainHandle>::new(config);
        let eth = Arc::new(registry.get_or_spawn(&self.eth_chain).unwrap_or_else(|e| {
            Output::error(format!("Forcerelay failed to start ethereum: {e}")).exit()
        }));
        let ckb = Arc::new(registry.get_or_spawn(&self.ckb_chain).unwrap_or_else(|e| {
            Output::error(format!("Forcerelay failed to start ckb: {e}")).exit()
        }));
        let eth_subscription = eth.subscribe().unwrap_or_else(|e| {
            Output::error(format!(
                "Forcerelay failed to subscribe ethereum events: {e}"
            ))
            .exit()
        });

        let src_chain = eth.clone();
        let dst_chain = ckb.clone();
        let handle = spawn_background_task(
            error_span!("worker.forcerelay"),
            Some(Duration::from_secs(5)),
            move || -> Result<Next, TaskError<Infallible>> {
                if let Ok(batch) = eth_subscription.try_recv() {
                    match batch.deref() {
                        Ok(batch) => {
                            handle_eth_ckb_event_batch(&src_chain, &dst_chain, batch);
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
        );

        tokio::runtime::Runtime::new()
            .expect("monitor tokio")
            .block_on(wait_shutdown(handle, eth, ckb));
    }
}

async fn wait_shutdown<ChainA: ChainHandle, ChainB: ChainHandle>(
    forcerelay: TaskHandle,
    eth: Arc<ChainA>,
    ckb: Arc<ChainB>,
) {
    let ctrl_c_handler = tokio::spawn(async {
        #[cfg(windows)]
        let _ = tokio::signal::ctrl_c().await;
        #[cfg(unix)]
        {
            use tokio::signal::unix as os_impl;
            let mut sigtun_int = os_impl::signal(os_impl::SignalKind::interrupt()).unwrap();
            let mut sigtun_term = os_impl::signal(os_impl::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = sigtun_int.recv() => {}
                _ = sigtun_term.recv() => {}
            };
        }
    });

    // register channel of panic
    let (panic_sender, mut panic_receiver) = tokio::sync::mpsc::channel(1);

    std::panic::set_hook(Box::new(move |info: &PanicInfo<'_>| {
        panic_sender
            .try_send(info.to_string())
            .expect("panic_receiver is droped");
    }));

    let shutdown = move || {
        eth.shutdown().expect("shutdown eth");
        ckb.shutdown().expect("shutdown ckb");
        forcerelay.shutdown_and_wait();
    };
    tokio::select! {
        _ = ctrl_c_handler => {
            warn!("<Ctrl-C> is pressed, quit Forcerelay and shutdown");
            shutdown();
        }
        Some(panic_info) = panic_receiver.recv() => {
            warn!("child thread paniced: {panic_info}");
            shutdown();
        }
    }
}
