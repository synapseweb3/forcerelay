pub mod gas_multiplier;
pub mod proof_specs;
pub mod types;

use core::time::Duration;

use ibc_proto::google::protobuf::Any;
use serde_derive::{Deserialize, Serialize};
use tendermint_light_client_verifier::types::TrustThreshold;

use ibc_relayer_types::core::ics23_commitment::specs::ProofSpecs;
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use ibc_relayer_types::timestamp::ZERO_DURATION;

use crate::error::Error as RelayerError;
use crate::keyring::Store;
use crate::{chain::ChainType, extension_options::ExtensionOptionDynamicFeeTx};

use super::{filter::PacketFilter, AddressType, GasPrice};
use gas_multiplier::GasMultiplier;
use types::{MaxMsgNum, MaxTxSize, Memo};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CosmosChainConfig {
    pub id: ChainId,
    #[serde(default = "default::chain_type")]
    pub r#type: ChainType,
    pub rpc_addr: tendermint_rpc::Url,
    pub websocket_addr: tendermint_rpc::Url,
    pub grpc_addr: tendermint_rpc::Url,
    #[serde(default = "default::rpc_timeout", with = "humantime_serde")]
    pub rpc_timeout: Duration,
    pub account_prefix: String,
    pub key_name: String,
    #[serde(default)]
    pub key_store_type: Store,
    pub store_prefix: String,
    pub default_gas: Option<u64>,
    pub max_gas: Option<u64>,

    // This field is deprecated, use `gas_multiplier` instead
    pub gas_adjustment: Option<f64>,
    pub gas_multiplier: Option<GasMultiplier>,

    pub fee_granter: Option<String>,
    #[serde(default)]
    pub max_msg_num: MaxMsgNum,
    #[serde(default)]
    pub max_tx_size: MaxTxSize,

    /// A correction parameter that helps deal with clocks that are only approximately synchronized
    /// between the source and destination chains for a client.
    /// This parameter is used when deciding to accept or reject a new header
    /// (originating from the source chain) for any client with the destination chain
    /// that uses this configuration, unless it is overridden by the client-specific
    /// clock drift option.
    #[serde(default = "default::clock_drift", with = "humantime_serde")]
    pub clock_drift: Duration,

    #[serde(default = "default::max_block_time", with = "humantime_serde")]
    pub max_block_time: Duration,

    /// The trusting period specifies how long a validator set is trusted for
    /// (must be shorter than the chain's unbonding period).
    #[serde(default, with = "humantime_serde")]
    pub trusting_period: Option<Duration>,

    #[serde(default)]
    pub memo_prefix: Memo,

    // Note: These last few need to be last otherwise we run into `ValueAfterTable` error when serializing to TOML.
    //       That's because these are all tables and have to come last when serializing.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "self::proof_specs"
    )]
    pub proof_specs: Option<ProofSpecs>,

    // This is an undocumented and hidden config to make the relayer wait for
    // DeliverTX before sending the next transaction when sending messages in
    // multiple batches. We will instruct relayer operators to turn this on
    // in case relaying failed in a chain with priority mempool enabled.
    // Warning: turning this on may cause degradation in performance.
    #[serde(default)]
    pub sequential_batch_tx: bool,

    // these two need to be last otherwise we run into `ValueAfterTable` error when serializing to TOML
    /// The trust threshold defines what fraction of the total voting power of a known
    /// and trusted validator set is sufficient for a commit to be accepted going forward.
    #[serde(default)]
    pub trust_threshold: TrustThreshold,

    pub gas_price: GasPrice,

    #[serde(default)]
    pub packet_filter: PacketFilter,

    #[serde(default)]
    pub address_type: AddressType,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub extension_options: Vec<ExtensionOption>,
}

/// Defaults for various fields
pub mod default {
    use super::*;

    pub fn chain_type() -> ChainType {
        ChainType::CosmosSdk
    }

    pub fn tx_confirmation() -> bool {
        false
    }

    pub fn clear_on_start() -> bool {
        true
    }

    pub fn clear_packets_interval() -> u64 {
        100
    }

    pub fn rpc_timeout() -> Duration {
        Duration::from_secs(10)
    }

    pub fn clock_drift() -> Duration {
        Duration::from_secs(5)
    }

    pub fn max_block_time() -> Duration {
        Duration::from_secs(30)
    }

    pub fn connection_delay() -> Duration {
        ZERO_DURATION
    }

    pub fn auto_register_counterparty_payee() -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "snake_case",
    tag = "type",
    content = "value",
    deny_unknown_fields
)]
pub enum ExtensionOption {
    EthermintDynamicFee(String),
}

impl ExtensionOption {
    pub fn to_any(&self) -> Result<Any, RelayerError> {
        match self {
            Self::EthermintDynamicFee(max_priority_price) => ExtensionOptionDynamicFeeTx {
                max_priority_price: max_priority_price.into(),
            }
            .to_any(),
        }
    }
}
