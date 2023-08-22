use ibc_test_framework::prelude::Error;

use crate::consts::{CHANNEL_TYPE_ARGS, CLIENT_TYPE_ARGS, CONNECTION_TYPE_ARGS, PACKET_TYPE_ARGS};
use crate::framework::binary::channel::run_arbitrary_binary_channel_test;
use crate::tests::channel::CKB4IbcChannelTest;

pub mod channel;

macro_rules! env_vars {
    ($({$key:expr, $val:expr},)+) => {
        $(std::env::set_var($key, $val);)+
    };
}

#[test]
fn test_config() {
    let path = "config.toml";
    relayer::config::load(path).unwrap();
}

#[test]
fn test_from_ckb_to_ckb() -> Result<(), Error> {
    env_vars!(
        {"CHAIN_COMMAND_PATHS", "ckb"},
        {"ACCOUNT_PREFIXES", "ckb"},
        {"CONNECTION_TYPE_ARGS", hex::encode(CONNECTION_TYPE_ARGS)},
        {"CHANNEL_TYPE_ARGS", hex::encode(CHANNEL_TYPE_ARGS)},
        {"PACKET_TYPE_ARGS", hex::encode(PACKET_TYPE_ARGS)},
        {"CLIENT_TYPE_ARGS", hex::encode(CLIENT_TYPE_ARGS)},
    );
    run_arbitrary_binary_channel_test(&CKB4IbcChannelTest)
}

#[ignore]
#[test]
fn test_from_axon_to_axon() -> Result<(), Error> {
    todo!()
}

#[ignore]
#[test]
fn test_from_axon_to_ckb() -> Result<(), Error> {
    todo!()
}

#[ignore]
#[test]
fn test_from_ckb_to_axon() -> Result<(), Error> {
    todo!()
}
