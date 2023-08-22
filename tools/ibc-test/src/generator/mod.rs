use ckb_types::{h256, H256};
use create_connection::generate_create_connection;
use deploy_channel::{generate_deploy_channel, ChannelAttribute};
use deploy_connection::{generate_deploy_connection, ConnectionAttribute};
use deploy_packet_metadata::{generate_deploy_packet_metadata, PacketMetataAttribute};

mod create_connection;
mod deploy_channel;
mod deploy_connection;
mod deploy_packet_metadata;
mod utils;

pub const PRIVKEY: &str = "63d86723e08f0f813a36ce6aa123bb2289d90680ae1e99d4de8cdb334553f24d";
pub const GENESIS_TXHASH: H256 =
    h256!("0x227de871ce6ab120a67960f831b04148bf79b4e56349dde7a8001f93191736ed");

#[ignore]
#[test]
fn generate() {
    let connetion_attr = generate_deploy_connection();
    let channel_attr = generate_deploy_channel(&connetion_attr);
    let packet_metadata_attr = generate_deploy_packet_metadata(&channel_attr);
    let (_, _) = generate_create_connection(&connetion_attr, &packet_metadata_attr);
    generate_consts_file(&connetion_attr, &channel_attr, &packet_metadata_attr);
}

fn generate_consts_file(
    connection_attr: &ConnectionAttribute,
    channel_attr: &ChannelAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) {
    let consts_rs = format!(
        r#"use ckb_types::{{h256, H256}};

pub const CONNECTION_CODE_HASH: H256 =
    h256!("0x{}");
pub const CHANNEL_CODE_HASH: H256 =
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
        connection_attr.connection_code_hash,
        channel_attr.channel_code_hash,
        connection_attr.connection_type_args,
        channel_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        packet_metadata_attr.metadata_type_args
    );
    std::fs::write("./src/consts.rs", consts_rs).unwrap();
}
