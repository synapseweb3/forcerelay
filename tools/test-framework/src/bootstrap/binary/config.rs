use ibc_relayer::config::{
    cosmos::ChainConfig as CosmosChainConfig, Config, GlobalConfig, ModeConfig, RestConfig,
    TelemetryConfig,
};
use serde_derive::{Deserialize, Serialize};

// A compatible config where chains' configs are Cosmos config. It is used to adapt to
// integration tests which serializes config into a toml file since `toml` can not serialize
// the enum type.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CosmosConfig {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default)]
    pub rest: RestConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<CosmosChainConfig>,
}

impl From<Config> for CosmosConfig {
    fn from(value: Config) -> Self {
        Self {
            global: value.global,
            mode: value.mode,
            rest: value.rest,
            telemetry: value.telemetry,
            chains: value
                .chains
                .into_iter()
                .map(|c| c.downcast_cosmos())
                .collect(),
        }
    }
}
