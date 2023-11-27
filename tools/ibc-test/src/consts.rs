use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0xa0989fbab44ff4e6c9d5be8497877275ca9724a92b77251495b4667251a67ebc");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0xeaf0835600aa99465a67fbfa98e90b829a0c06fd94cc526e71c91f82fa667199");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0xdf1e96079d0aaa5798163bbadd4f0d5267ea3264c40e94b633a4e164d6ca7983");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x289d590a52f63458cc9b70cdd4b962c1198be73438d5968dba80ed76daf25b3c");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
