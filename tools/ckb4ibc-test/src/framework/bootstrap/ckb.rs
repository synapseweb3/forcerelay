use ibc_test_framework::chain::chain_type::ChainType;
use ibc_test_framework::types::process::ChildProcess;

use std::sync::{Arc, RwLock};

use toml;

use crate::framework::utils::{self, ckb::*};
use ibc_test_framework::chain::builder::ChainBuilder;

use ibc_test_framework::chain::driver::ChainDriver;
use ibc_test_framework::chain::ext::bootstrap::ChainBootstrapMethodsExt;
use ibc_test_framework::error::Error;
use ibc_test_framework::ibc::denom::Denom;

use ibc_test_framework::prelude::*;
use ibc_test_framework::types::single::node::FullNode;
use ibc_test_framework::types::wallet::{TestWallets, Wallet};
use ibc_test_framework::util::random::random_u32;

pub fn bootstrap_single_ckb_node(
    builder: &ChainBuilder,
    prefix: &str,
    use_random_id: bool,
    _config_modifier: impl FnOnce(&mut toml::Value) -> Result<(), Error>,
    _genesis_modifier: impl FnOnce(&mut serde_json::Value) -> Result<(), Error>,
    chain_number: usize,
) -> Result<(FullNode, ChildProcess), Error> {
    let chain_driver = builder.new_chain(prefix, use_random_id, chain_number)?;
    assert_eq!(chain_driver.chain_type, ChainType::Ckb);

    info!(
        "started new chain {} at with home path {} and RPC address {}.",
        chain_driver.chain_id,
        chain_driver.home_path,
        chain_driver.rpc_address(),
    );

    // FIXME @jjy
    // currently the deploy tx is hard coded, which means relayer must be a fixed pk
    let (process, miner_process) = prepare_ckb_chain(
        &chain_driver.home_path,
        chain_driver.rpc_port as u32,
        &[],
        // &[(&relayer, 5_198_735_037_00000000u64)],
    );

    let validator = add_wallet(&chain_driver, "validator", use_random_id)?;
    let user1 = add_wallet(&chain_driver, "user1", use_random_id)?;
    let user2 = add_wallet(&chain_driver, "user2", use_random_id)?;
    // FIXME @jjy use random pk as relayer once remove hardcoded deploy transactions
    // let relayer = add_wallet(&chain_driver, "relayer", use_random_id)?;
    let relayer = add_ckb_devnet_relayer_wallet(&chain_driver, "relayer", use_random_id)?;

    info!(
        "user wallet for chain {} - id: {}, address: {}",
        chain_driver.chain_id, user1.id.0, user1.address.0,
    );

    info!(
        "you can manually interact with the chain using commands starting with:\n{} --home '{}' --node {}",
        chain_driver.command_path,
        chain_driver.home_path,
        chain_driver.rpc_address(),
    );

    let wallets = TestWallets {
        validator,
        relayer,
        user1,
        user2,
    };

    let denom = Denom::base("ckb");

    let node = FullNode {
        chain_driver,
        denom,
        wallets,
        process: Arc::new(RwLock::new(process)),
    };

    Ok((node, miner_process))
}

fn add_wallet(driver: &ChainDriver, prefix: &str, use_random_id: bool) -> Result<Wallet, Error> {
    match driver.chain_type {
        ChainType::Ckb => utils::ckb::add_ckb_wallet(driver, prefix, use_random_id),
        _ => {
            if use_random_id {
                let num = random_u32();
                let wallet_id = format!("{prefix}-{num:x}");
                driver.add_wallet(&wallet_id)
            } else {
                driver.add_wallet(prefix)
            }
        }
    }
}
