//! Data structures and logic to set up IBC client's parameters.

use crate::chain::cosmos;
use crate::config::ChainConfig;
use crate::foreign_client::CreateOptions;

use super::ChainType;

/// Client parameters for the `build_create_client` operation.
///
/// The parameters are specialized for each supported chain type.
#[derive(Clone, Debug)]
pub enum ClientSettings {
    Tendermint(cosmos::client::Settings),
    AxonCkb,
    Other,
}

impl ClientSettings {
    /// Takes the settings from the user-supplied options if they have been specified,
    /// falling back to defaults using the configuration of the source
    /// and the destination chain.
    pub fn for_create_command(
        options: CreateOptions,
        src_chain_config: &ChainConfig,
        dst_chain_config: &ChainConfig,
    ) -> Self {
        match (src_chain_config.r#type(), dst_chain_config.r#type()) {
            (ChainType::CosmosSdk, ChainType::CosmosSdk) => {
                // Currently, only Tendermint chain pairs are supported by
                // ForeignClient::build_create_client_and_send. Support for
                // heterogeneous chains is left for future revisions.
                ClientSettings::Tendermint(cosmos::client::Settings::for_create_command(
                    options,
                    src_chain_config,
                    dst_chain_config,
                ))
            }
            (ChainType::Axon, ChainType::Ckb4Ibc) | (ChainType::Ckb4Ibc, ChainType::Axon) => {
                ClientSettings::AxonCkb
            }
            _ => ClientSettings::Other,
        }
    }
}
