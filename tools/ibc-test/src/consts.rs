use ckb_types::{h160, h256, H160, H256};

pub const SUDT_CODE_HASH: H256 =
    h256!("0xcf6e0c0148123081af1deda0ef162d39cfdfe1ea6565d3689009c1f3562a5e82");
pub const CONNECTION_CODE_HASH: H256 =
    h256!("0xe802fa98adeed078eb39225f6d7759ceeeecc32fa28942f2d2c6a158ec40cebb");
pub const CHANNEL_CODE_HASH: H256 =
    h256!("0xa666ecfbe673fe3bfb7a60ce52db78da2f24bc3d65118906e5ef96d299e81bf8");

pub const SUDT_TYPE_ARGS: H256 =
    h256!("0xf49ce32397c6741998b04d7548c5ed372007424daf67ee5bfadaefec3c865781");
pub const CONNECTION_TYPE_ARGS: H256 =
    h256!("0x9d46ad77b8f5107d5898d7e7ad82be206c4fd95f562004efe32488605e8846c7");
pub const CHANNEL_TYPE_ARGS: H256 =
    h256!("0x50179971499a451d06059365ae559329afcc48a3b7a53af5310d906c0e22f16d");
pub const PACKET_TYPE_ARGS: H256 =
    h256!("0x901959f4e4b8b8c8aa79bdad1e68742d73bcd7f510599249bb46c1923178bd0f");
pub const CLIENT_TYPE_ARGS: H256 =
    h256!("0x2f3efdcdc1ae5327187920eec83da80bdaa11c581d47aee0eabc895e9ac713e8");

// Don't know how to pass this from axon to ckb. It doesn't change often, so
// let's hardcode it for now.
pub const AXON_IBC_HANDLER_ADDRESS: H160 = h160!("0xdc64a140aa3e981100a9beca4e685f962f0cf6c9");
