use ckb_sdk::constants::TYPE_ID_CODE_HASH;
use ibc_test_framework::prelude::Error;

use crate::consts::{CHANNEL_TYPE_ARGS, CLIENT_TYPE_ARGS, CONNECTION_TYPE_ARGS, PACKET_TYPE_ARGS};
use crate::framework::binary::channel::run_arbitrary_binary_channel_test;
use crate::tests::{channel::ChannelTest, packet::CKB4IbcPacketTest};

mod channel;
mod packet;

macro_rules! env_vars {
    ($({$key:expr, $val:expr},)+) => {
        $(std::env::set_var($key, $val);)+
    };
}

#[test]
fn matrix_test_within_axon_and_ckb() -> Result<(), Error> {
    let value = std::env::var("ACCOUNT_PREFIXES")
        .map_err(|e| eyre::eyre!("no ENV entry \"ACCOUNT_PREFIXES\": {e}"))?;
    let mut packet_run = false;
    if value.contains("ckb") {
        env_vars!(
            {"CLIENT_CODE_HASH", hex::encode(TYPE_ID_CODE_HASH)},
            {"CONNECTION_TYPE_ARGS", hex::encode(CONNECTION_TYPE_ARGS)},
            {"CHANNEL_TYPE_ARGS", hex::encode(CHANNEL_TYPE_ARGS)},
            {"PACKET_TYPE_ARGS", hex::encode(PACKET_TYPE_ARGS)},
            {"CLIENT_TYPE_ARGS", hex::encode(CLIENT_TYPE_ARGS)},
        );
        if value == "ckb,ckb" {
            packet_run = true;
        }
    }
    run_arbitrary_binary_channel_test(&ChannelTest::new(&CKB4IbcPacketTest::new(packet_run)))
}

#[ignore]
#[test]
fn specific_test_only_for_ckb() -> Result<(), Error> {
    env_vars!(
        {"CHAIN_COMMAND_PATHS", "ckb,ckb"},
        {"ACCOUNT_PREFIXES", "ckb,ckb"},
        {"CLIENT_CODE_HASH", hex::encode(TYPE_ID_CODE_HASH)},
        {"CONNECTION_TYPE_ARGS", hex::encode(CONNECTION_TYPE_ARGS)},
        {"CHANNEL_TYPE_ARGS", hex::encode(CHANNEL_TYPE_ARGS)},
        {"PACKET_TYPE_ARGS", hex::encode(PACKET_TYPE_ARGS)},
        {"CLIENT_TYPE_ARGS", hex::encode(CLIENT_TYPE_ARGS)},
    );
    run_arbitrary_binary_channel_test(&ChannelTest::new(&CKB4IbcPacketTest::new(true)))
}
