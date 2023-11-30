use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0xf2a531035a5addf149f7eccadb0f33d0695590d5267fa8be51c783a448e1bfab");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0xf91561ebb15317f768bf077d3bff174f324a575b470b7a0b5cbb43e0c3dfbe58");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x4ea2bd6ade53af0537b6913e68f3f8111bac56122b7c78fd894015ed72d5f0e0");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0xe390aa75c18276d35c40bfd69d8310c6d3243831241a41ef4d3ce1a56c5cd1fe");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
