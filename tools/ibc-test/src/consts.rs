use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0xb73a1a973b853522f872457162df66d14db6cc3d22d3305200b230b1e8cf3a49");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0x3b823e9d38e4fc71cfcf41db014e9c131d33f7b85878ea1e08b77ad1a585eba3");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x587a81b5a0cc803816230b344cb5a0658c8bfc1ba16991481b2e6789c26f241c");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x7395c8933c8a0438c9b64d4325fd10a8357846c45d3c3bd3ae5467b85de9f731");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
