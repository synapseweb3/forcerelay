use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0x96f3022f50114ed44780fb3aa09301a4a73dd75ad1aa229dbf0a07328df78e4d");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0xf58987500f12e00e8654ed94bae356838cce1bb477f4f4affa91070a9d60aa5c");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x6d19037a78abf44724d3077794f3bc756aaec08aa2999f37834eb4c4a054d89f");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x24517cd123807894994d5e9d572c3a786f565af4c46fa33543f58cec99b0f9c8");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
