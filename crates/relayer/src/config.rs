//! Relayer configuration
pub mod ckb;
pub mod cosmos;
pub mod error;
pub mod eth;
pub mod filter;

use alloc::collections::BTreeMap;
use core::{
    cmp::Ordering,
    fmt::{Display, Error as FmtError, Formatter},
    str::FromStr,
    time::Duration,
};
use std::{fs, fs::File, io::Write, path::Path};

use ibc_proto::google::protobuf::Any;
use serde_derive::{Deserialize, Serialize};

use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc_relayer_types::timestamp::ZERO_DURATION;

use crate::chain::ChainType;
use crate::error::Error as RelayerError;
use crate::extension_options::ExtensionOptionDynamicFeeTx;

pub use crate::config::Error as ConfigError;
use ckb::ChainConfig as CkbChainConfig;
use cosmos::CosmosChainConfig;
pub use error::Error;
use eth::EthChainConfig;

use self::filter::PacketFilter;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GasPrice {
    pub price: f64,
    pub denom: String,
}

impl GasPrice {
    pub const fn new(price: f64, denom: String) -> Self {
        Self { price, denom }
    }
}

impl Display for GasPrice {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(f, "{}{}", self.price, self.denom)
    }
}

impl FromStr for GasPrice {
    type Err = ConfigError;

    fn from_str(price_in: &str) -> Result<Self, Self::Err> {
        // TODO: We split by `char::is_alphabetic` delimiter.
        //       More robust parsing methods might be needed.
        let spos = price_in.find(char::is_alphabetic);

        match spos {
            Some(position) => {
                let (price_str, denom) = price_in.split_at(position);

                let price = price_str
                    .parse::<f64>()
                    .map_err(|_| Error::invalid_gas_price(price_in.to_string()))?;

                Ok(GasPrice {
                    price,
                    denom: denom.to_owned(),
                })
            }

            None => Err(Error::invalid_gas_price(price_in.to_string())),
        }
    }
}

// Note: Only `PartialOrd` is implemented for `GasPrice` because gas
// prices must be of the same denomination in order to be compared.
impl PartialOrd for GasPrice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.denom == other.denom {
            self.price.partial_cmp(&other.price)
        } else {
            None
        }
    }
}

/// Attempts to parse 0 or more `GasPrice`s from a String,
/// returning the successfully parsed prices in a Vec. Any
/// single price that fails to be parsed does not affect
/// the parsing of other prices.
pub fn parse_gas_prices(prices: String) -> Vec<GasPrice> {
    prices
        .split(';')
        .filter_map(|gp| GasPrice::from_str(gp).ok())
        .collect()
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

impl Display for ExtensionOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::EthermintDynamicFee(max_priority_price) => {
                write!(
                    f,
                    "EthermintDynamicFee(max_priority_price: {max_priority_price})"
                )
            }
        }
    }
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

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ChainConfig {
    Cosmos(CosmosChainConfig),
    Eth(EthChainConfig),
    Ckb(CkbChainConfig),
}

impl ChainConfig {
    pub fn id(&self) -> &ChainId {
        match self {
            ChainConfig::Cosmos(c) => &c.id,
            ChainConfig::Eth(c) => &c.id,
            ChainConfig::Ckb(c) => &c.id,
        }
    }

    pub fn packet_filter(&self) -> &PacketFilter {
        match self {
            ChainConfig::Cosmos(c) => &c.packet_filter,
            ChainConfig::Eth(_) => todo!(),
            ChainConfig::Ckb(_) => todo!(),
        }
    }

    pub fn key_name(&self) -> &str {
        match self {
            ChainConfig::Cosmos(c) => &c.key_name,
            ChainConfig::Eth(c) => &c.key_name,
            ChainConfig::Ckb(c) => &c.key_name,
        }
    }

    pub fn downcast_cosmos(self) -> CosmosChainConfig {
        if let ChainConfig::Cosmos(c) = self {
            c
        } else {
            panic!("Not a cosmos chain")
        }
    }

    pub fn cosmos_mut(&mut self) -> &mut CosmosChainConfig {
        if let ChainConfig::Cosmos(c) = self {
            c
        } else {
            panic!("Not a cosmos chain")
        }
    }

    pub fn cosmos(&self) -> &CosmosChainConfig {
        if let ChainConfig::Cosmos(c) = self {
            c
        } else {
            panic!("Not a cosmos chain")
        }
    }

    pub fn eth(&self) -> &EthChainConfig {
        if let ChainConfig::Eth(e) = self {
            e
        } else {
            panic!("Not a eth chain")
        }
    }

    pub fn r#type(&self) -> ChainType {
        match self {
            ChainConfig::Cosmos(_) => ChainType::CosmosSdk,
            ChainConfig::Eth(_) => ChainType::Eth,
            ChainConfig::Ckb(_) => ChainType::Ckb,
        }
    }

    pub fn max_block_time(&self) -> Duration {
        match self {
            ChainConfig::Cosmos(c) => c.max_block_time,
            ChainConfig::Eth(_) => todo!(),
            ChainConfig::Ckb(_) => todo!(),
        }
    }
}

impl<'a> TryFrom<&'a ChainConfig> for &'a CosmosChainConfig {
    type Error = RelayerError;

    fn try_from(value: &'a ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Cosmos(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Cosmos config".to_owned()),
            )))
        }
    }
}

impl TryFrom<ChainConfig> for CosmosChainConfig {
    type Error = RelayerError;

    fn try_from(value: ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Cosmos(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Cosmos config".to_owned()),
            )))
        }
    }
}

impl<'a> TryFrom<&'a ChainConfig> for &'a EthChainConfig {
    type Error = RelayerError;

    fn try_from(value: &'a ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Eth(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Ethereum config".to_owned()),
            )))
        }
    }
}

impl TryFrom<ChainConfig> for EthChainConfig {
    type Error = RelayerError;

    fn try_from(value: ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Eth(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Ethereum config".to_owned()),
            )))
        }
    }
}

impl<'a> TryFrom<&'a ChainConfig> for &'a CkbChainConfig {
    type Error = RelayerError;

    fn try_from(value: &'a ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Ckb(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Ckb config".to_owned()),
            )))
        }
    }
}

impl TryFrom<ChainConfig> for CkbChainConfig {
    type Error = RelayerError;

    fn try_from(value: ChainConfig) -> Result<Self, Self::Error> {
        if let ChainConfig::Ckb(value) = value {
            Ok(value)
        } else {
            Err(RelayerError::config(ConfigError::encode(
                toml::ser::Error::Custom("not Ckb config".to_owned()),
            )))
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default)]
    pub rest: RestConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<ChainConfig>,
}

impl Config {
    pub fn has_chain(&self, id: &ChainId) -> bool {
        self.chains.iter().any(|c| c.id() == id)
    }

    pub fn find_chain(&self, id: &ChainId) -> Option<&ChainConfig> {
        self.chains.iter().find(|c| c.id() == id)
    }

    pub fn find_chain_mut(&mut self, id: &ChainId) -> Option<&mut ChainConfig> {
        self.chains.iter_mut().find(|c| c.id() == id)
    }

    /// Returns true if filtering is disabled or if packets are allowed on
    /// the channel [`PortId`] [`ChannelId`] on [`ChainId`].
    /// Returns false otherwise.
    pub fn packets_on_channel_allowed(
        &self,
        chain_id: &ChainId,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> bool {
        match self.find_chain(chain_id) {
            Some(chain_config) => {
                if !matches!(chain_config, ChainConfig::Cosmos(_)) {
                    false
                } else {
                    chain_config.packet_filter().is_allowed(port_id, channel_id)
                }
            }
            None => false,
        }
    }

    pub fn chains_map(&self) -> BTreeMap<&ChainId, &ChainConfig> {
        self.chains.iter().map(|c| (c.id(), c)).collect()
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeConfig {
    pub clients: Clients,
    pub connections: Connections,
    pub channels: Channels,
    pub packets: Packets,
}

impl ModeConfig {
    pub fn all_disabled(&self) -> bool {
        !self.clients.enabled
            && !self.connections.enabled
            && !self.channels.enabled
            && !self.packets.enabled
    }
}

/// # IMPORTANT: Keep the values here in sync with the values in the default config.toml.
impl Default for ModeConfig {
    fn default() -> Self {
        Self {
            clients: Clients {
                enabled: true,
                refresh: true,
                misbehaviour: false,
            },
            connections: Connections { enabled: false },
            channels: Channels { enabled: false },
            packets: Packets {
                enabled: true,
                ..Default::default()
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Clients {
    pub enabled: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default)]
    pub misbehaviour: bool,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Connections {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Channels {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Packets {
    pub enabled: bool,
    #[serde(default = "default::clear_packets_interval")]
    pub clear_interval: u64,
    #[serde(default = "default::clear_on_start")]
    pub clear_on_start: bool,
    #[serde(default = "default::tx_confirmation")]
    pub tx_confirmation: bool,
    #[serde(default = "default::auto_register_counterparty_payee")]
    pub auto_register_counterparty_payee: bool,
}

impl Default for Packets {
    fn default() -> Self {
        Self {
            enabled: true,
            clear_interval: default::clear_packets_interval(),
            clear_on_start: default::clear_on_start(),
            tx_confirmation: default::tx_confirmation(),
            auto_register_counterparty_payee: default::auto_register_counterparty_payee(),
        }
    }
}

/// Log levels are wrappers over [`tracing_core::Level`].
///
/// [`tracing_core::Level`]: https://docs.rs/tracing-core/0.1.17/tracing_core/struct.Level.html
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl Display for LogLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
    pub log_level: LogLevel,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

/// Default values for the telemetry configuration.
///
/// # IMPORTANT: Remember to update the Hermes guide & the default config.toml whenever these values change.
impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 3001,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RestConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
    }
}

/// It defines the address generation method
/// TODO: Ethermint `pk_type` to be restricted
/// after the Cosmos SDK release with ethsecp256k1
/// <https://github.com/cosmos/cosmos-sdk/pull/9981>
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    rename_all = "lowercase",
    tag = "derivation",
    content = "proto_type",
    deny_unknown_fields
)]
pub enum AddressType {
    Cosmos,
    Ethermint { pk_type: String },
    Ckb { is_mainnet: bool },
}

impl Default for AddressType {
    fn default() -> Self {
        AddressType::Cosmos
    }
}

impl Display for AddressType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match self {
            AddressType::Cosmos => write!(f, "cosmos"),
            AddressType::Ethermint { .. } => write!(f, "ethermint"),
            AddressType::Ckb { .. } => write!(f, "ckb"),
        }
    }
}

/// Attempt to load and parse the TOML config file as a `Config`.
pub fn load(path: impl AsRef<Path>) -> Result<Config, Error> {
    let config_toml = std::fs::read_to_string(&path).map_err(Error::io)?;

    let config = toml::from_str::<Config>(&config_toml[..]).map_err(Error::decode)?;

    Ok(config)
}

/// Serialize the given `Config` as TOML to the given config file.
pub fn store(config: &Config, path: impl AsRef<Path>) -> Result<(), Error> {
    let mut file = if path.as_ref().exists() {
        fs::OpenOptions::new().write(true).truncate(true).open(path)
    } else {
        File::create(path)
    }
    .map_err(Error::io)?;

    store_writer(config, &mut file)
}

/// Serialize the given `Config` as TOML to the given writer.
pub(crate) fn store_writer(config: &Config, mut writer: impl Write) -> Result<(), Error> {
    let toml_config = toml::to_string_pretty(&config).map_err(Error::encode)?;

    writeln!(writer, "{toml_config}").map_err(Error::io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use super::{load, parse_gas_prices, store_writer};
    use crate::config::GasPrice;
    use test_log::test;

    #[test]
    fn parse_valid_config() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/config/fixtures/relayer_conf_example.toml"
        );

        let config = load(path).expect("could not parse config");

        dbg!(config);
    }

    #[test]
    fn serialize_valid_config() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/config/fixtures/relayer_conf_example.toml"
        );

        let config = load(path).expect("could not parse config");

        let mut buffer = Vec::new();
        store_writer(&config, &mut buffer).unwrap();
    }

    #[test]
    fn gas_price_from_str() {
        let gp_original = GasPrice::new(10.0, "atom".to_owned());

        let gp_raw = gp_original.to_string();
        let gp = GasPrice::from_str(&gp_raw).expect("could not parse String into GasPrice");

        assert_eq!(gp, gp_original);
    }

    #[test]
    fn parse_multiple_gas_prices() {
        let gas_prices = "0.25token1;0.0001token2";
        let parsed = parse_gas_prices(gas_prices.to_string());

        let expected = vec![
            GasPrice {
                price: 0.25,
                denom: "token1".to_owned(),
            },
            GasPrice {
                price: 0.0001,
                denom: "token2".to_owned(),
            },
        ];

        assert_eq!(expected, parsed);
    }

    #[test]
    fn parse_empty_gas_price() {
        let empty_price = "";
        let parsed = parse_gas_prices(empty_price.to_string());

        assert_eq!(parsed, vec![]);
    }

    #[test]
    fn malformed_gas_prices_do_not_get_parsed() {
        let malformed_prices = "token1;.token2;0.25token3";
        let parsed = parse_gas_prices(malformed_prices.to_string());

        let expected = vec![GasPrice {
            price: 0.25,
            denom: "token3".to_owned(),
        }];

        assert_eq!(expected, parsed);
    }
}
