use super::{
    deploy_channel::ChannelAttribute, deploy_connection::ConnectionAttribute,
    deploy_packet_metadata::PacketMetataAttribute,
};

const PREFIX: &str = r#"[global]
log_level = 'info'

[mode]

[mode.clients]
enabled = true
refresh = true
misbehaviour = true

[mode.connections]
enabled = false

[mode.channels]
enabled = false

[mode.packets]
enabled = true
clear_interval = 100
clear_on_start = true
tx_confirmation = true
"#;

pub fn generate_test_config(
    connection_attr: &ConnectionAttribute,
    channel_attr: &ChannelAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) {
    let chain_a = format!(
        r#"
[[chains]]
id = "{}"
ckb_rpc = "http://127.0.0.1:{}"
ckb_indexer_rpc = "http://127.0.0.1:{}"
key_name = "relayer_ckb_wallet"
store_prefix = "forcerelay"
connection_type_args = "0x{}"
channel_type_args = "0x{}"
packet_type_args = "0x{}"
[chains.onchain_light_clients]
Ckb4Ibc = {{ chain_id = "{}", client_cell_type_args = "0x{}" }}
"#,
        "ckb4ibc-0",
        8114,
        8114,
        connection_attr.connection_type_args,
        channel_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        "ckb4ibc-1",
        packet_metadata_attr.metadata_type_args,
    );
    let chain_b = format!(
        r#"
[[chains]]
id = "{}"
ckb_rpc = "http://127.0.0.1:{}"
ckb_indexer_rpc = "http://127.0.0.1:{}"
key_name = "relayer_ckb_wallet"
store_prefix = "forcerelay"
connection_type_args = "0x{}"
channel_type_args = "0x{}"
packet_type_args = "0x{}"
[chains.onchain_light_clients]
Ckb4Ibc = {{ chain_id = "{}", client_cell_type_args = "0x{}" }}
"#,
        "ckb4ibc-1",
        8214,
        8214,
        connection_attr.connection_type_args,
        channel_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        "ckb4ibc-0",
        packet_metadata_attr.metadata_type_args,
    );

    let test_config_content = format!("{}{}{}", PREFIX, chain_a, chain_b);
    std::fs::write("config.toml", test_config_content).unwrap();
}

pub fn generate_consts_file(
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
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x{}");
"#,
        connection_attr.connection_code_hash,
        channel_attr.channel_code_hash,
        packet_metadata_attr.metadata_type_args
    );
    std::fs::write("./src/consts.rs", consts_rs).unwrap();
}
