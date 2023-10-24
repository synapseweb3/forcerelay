use ckb_types::{h256, H256};
use deploy_channel::{generate_deploy_channel, ChannelAttribute};
use deploy_connection::{generate_deploy_connection, ConnectionAttribute};
use deploy_packet_metadata::{generate_deploy_packet_metadata, PacketMetataAttribute};
use issue_sudt::generate_create_sudt_escrow;

mod deploy_channel;
mod deploy_connection;
mod deploy_packet_metadata;
mod deploy_sudt;
mod issue_sudt;
mod utils;

pub use utils::{calc_script_hash, get_lock_script};

use self::deploy_sudt::{generate_deploy_sudt, SUDTAttribute};

pub const PRIVKEY: &str = "63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d";
pub const GENESIS_TXHASH: H256 =
    h256!("0x227de871ce6ab120a67960f831b04148bf79b4e56349dde7a8001f93191736ed");

#[ignore]
#[test]
fn generate() {
    let sudt_attr = generate_deploy_sudt();
    let connection_attr = generate_deploy_connection(&sudt_attr);
    let channel_attr = generate_deploy_channel(&connection_attr);
    let packet_metadata_attr = generate_deploy_packet_metadata(&channel_attr);
    let (_, _) = generate_create_sudt_escrow(&sudt_attr, &packet_metadata_attr);
    generate_consts_file(
        &sudt_attr,
        &connection_attr,
        &channel_attr,
        &packet_metadata_attr,
    );
}

fn generate_consts_file(
    sudt_attr: &SUDTAttribute,
    connection_attr: &ConnectionAttribute,
    channel_attr: &ChannelAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) {
    let consts_rs = format!(
        r#"use ckb_types::{{h256, H256}};

pub const SUDT_CODE_HASH: H256 =
    h256!("0x{}");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0x{}");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0x{}");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0x{}");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x{}");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0x{}");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x{}");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x{}");
"#,
        sudt_attr.sudt_code_hash,
        connection_attr.connection_code_hash,
        channel_attr.channel_code_hash,
        sudt_attr.sudt_type_args,
        connection_attr.connection_type_args,
        channel_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        packet_metadata_attr.metadata_type_args
    );
    std::fs::write("./src/consts.rs", consts_rs).unwrap();
}
