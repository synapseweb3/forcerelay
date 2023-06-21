use super::{deploy_conn_chan::ConnChanAttribute, deploy_packet_metadata::PacketMetataAttribute};

pub fn generate_test_config(
    conn_chan_attr: &ConnChanAttribute,
    packet_metadata_attr: &PacketMetataAttribute,
) {
    let chain_a = format!(
        r#"
[[chains]]
id = "{}"
ckb_rpc = "http://127.0.0.1:{}"
ckb_indexer_rpc = "http://127.0.0.1:{}"
key_name = "relayer_ckb_wallet"
counter_chain = "{}"

connection_type_args = "0x{}"
channel_type_args = "0x{}"
packet_type_args = "0x{}"
client_type_args = "0x{}"
"#,
        "ckb4ibc-0",
        8114,
        8114,
        "ckb4ibc-1",
        conn_chan_attr.connection_type_args,
        conn_chan_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        packet_metadata_attr.metadata_type_args
    );
    let chain_b = format!(
        r#"
[[chains]]
id = "{}"
ckb_rpc = "http://127.0.0.1:{}"
ckb_indexer_rpc = "http://127.0.0.1:{}"
key_name = "relayer_ckb_wallet"
counter_chain = "{}"

connection_type_args = "0x{}"
channel_type_args = "0x{}"
packet_type_args = "0x{}"
client_type_args = "0x{}"
"#,
        "ckb4ibc-1",
        8214,
        8214,
        "ckb4ibc-0",
        conn_chan_attr.connection_type_args,
        conn_chan_attr.channel_type_args,
        packet_metadata_attr.packet_type_args,
        packet_metadata_attr.metadata_type_args
    );

    let test_config_content = format!("{}{}{}", PREFIX, chain_a, chain_b);
    std::fs::write("config.toml", test_config_content).unwrap();
}

const PREFIX: &str = r#"[global]
log_level = 'error'

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
