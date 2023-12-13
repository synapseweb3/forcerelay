use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0x9b510952dd51cd5dec2407cac46b930ca48a9f4b0c74c0c80e4f603c4932761b");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0x7e9ec14af541437f96b0e47c3d2b6daa0d00d860c695fd7339bf5437b9276b7a");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x6a267987e0fa1467f45c724ef2301d5063e17c3e6834f318bcc03f9a6f28c5a6");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x00e2d9bb5e4d533d6757615a1cae35b5cfb8964c82b6055941d02d98265061c6");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
