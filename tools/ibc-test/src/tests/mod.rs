use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ibc_test_framework::prelude::Error;

use crate::consts::*;
use crate::framework::binary::channel::{
    run_arbitrary_binary_channel_test, run_arbitrary_binary_connection_test,
};

pub mod ckb;
mod ibc;

macro_rules! env_vars {
    ($({$key:expr, $val:expr},)+) => {
        $(std::env::set_var($key, $val);)+
    };
}

fn init_envs() -> Result<(), Error> {
    let value = std::env::var("ACCOUNT_PREFIXES")
        .map_err(|e| eyre::eyre!("no ENV entry \"ACCOUNT_PREFIXES\": {e}"))?;
    if value.contains("ckb") {
        env_vars!(
            {"CLIENT_CODE_HASH", hex::encode(TYPE_ID_CODE_HASH)},
            {"CONNECTION_TYPE_ARGS", hex::encode(CONNECTION_TYPE_ARGS)},
            {"CHANNEL_TYPE_ARGS", hex::encode(CHANNEL_TYPE_ARGS)},
            {"PACKET_TYPE_ARGS", hex::encode(PACKET_TYPE_ARGS)},
            {"CLIENT_TYPE_ARGS", hex::encode(CLIENT_TYPE_ARGS)},
            {"AXON_IBC_HANDLER_ADDRESS", hex::encode(AXON_IBC_HANDLER_ADDRESS)},
        );
    }
    Ok(())
}

#[test]
fn test_channel() -> Result<(), Error> {
    init_envs()?;
    run_arbitrary_binary_channel_test(&ibc::channel::ChannelTest::new())
}

#[test]
fn test_transfer() -> Result<(), Error> {
    init_envs()?;
    run_arbitrary_binary_channel_test(&ibc::transfer::TransferTest::new())
}

#[test]
fn test_ckb_packet() -> Result<(), Error> {
    init_envs()?;
    let value = std::env::var("ACCOUNT_PREFIXES")
        .map_err(|e| eyre::eyre!("no ENV entry \"ACCOUNT_PREFIXES\": {e}"))?;
    if value == "ckb,ckb" {
        log::info!("Run ckb packet tests for {}", value);
        run_arbitrary_binary_channel_test(&ckb::packet::CKB4IbcPacketTest::new())
    } else {
        log::info!("Ignore ckb packet tests for {}", value);
        Ok(())
    }
}

#[test]
fn test_sudt_erc20_transfer() -> Result<(), Error> {
    init_envs()?;
    let value = std::env::var("ACCOUNT_PREFIXES")
        .map_err(|e| eyre::eyre!("no ENV entry \"ACCOUNT_PREFIXES\": {e}"))?;
    if value == "ckb,axon" {
        println!("Run sudt erc20 transfer test for {}", value);
        run_arbitrary_binary_connection_test(&ibc::sudt_erc20_transfer::SudtErc20TransferTest)
    } else {
        println!("Ignore sudt erc20 transfer test for {}", value);
        Ok(())
    }
}

#[ignore = "local dev manually test"]
#[test]
fn specific_test_only_for_ckb() -> Result<(), Error> {
    env_vars!(
        {"CHAIN_COMMAND_PATHS", "ckb,ckb"},
        {"ACCOUNT_PREFIXES", "ckb,ckb"},
    );
    init_envs()?;
    run_arbitrary_binary_channel_test(&ckb::packet::CKB4IbcPacketTest::new())
}
