use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0x4eb3ffbbf4a2b0a14da17d4951860ab6ae6ebd654515ea58cdb5a62b4c0c5cf7");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0x39f75db1c443ef884dac56482bf2912054cf7989eaf2c71cf44b0df91cdd86b0");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x56bd6db5728d711559bda25121e13e1d5d39157ac62b45824b87534b3228d3e8");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0xcdaf5ce4d6f41c98b7abaf417b867fe78778432eabaf06ee174144e63195c2d6");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
