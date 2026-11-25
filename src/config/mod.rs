//! Configuration types for the supported CLIProxyAPI v8 fields.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

mod store;
mod validation;

pub use store::ConfigError;

const CONFIG_VERSION: u32 = 8;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    pub config_version: u32,
    pub server: ServerConfig,
    pub management: ManagementConfig,
    pub access: AccessConfig,
    pub api_keys: UpstreamApiKeys,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            server: ServerConfig::default(),
            management: ManagementConfig::default(),
            access: AccessConfig::default(),
            api_keys: UpstreamApiKeys::default(),
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(deserialize_with = "deserialize_server_id")]
    pub server_id: String,
}

fn deserialize_server_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ManagementConfig {
    pub allow_remote: bool,
    pub secret_key: String,
    pub disable_control_panel: bool,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct AccessConfig {
    /// Client keys used to authenticate requests to this proxy.
    pub api_keys: Vec<String>,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct UpstreamApiKeys {
    #[serde(deserialize_with = "deserialize_group_entries")]
    pub codex: Vec<CodexGroup>,
    #[serde(deserialize_with = "deserialize_group_entries")]
    pub openai_compatibility: Vec<OpenAiCompatibilityGroup>,
}

/// Credentials and model mappings for a Responses upstream.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexGroup {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub models: Vec<ModelMapping>,
    #[serde(deserialize_with = "deserialize_group_entries")]
    pub keys: Vec<CodexApiKey>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CodexApiKey {
    #[serde(default)]
    pub api_key: String,
    pub proxy_url: Option<String>,
    /// An omitted list inherits the group mappings. An empty list overrides them.
    pub models: Option<Vec<ModelMapping>>,
}

/// Credentials and model mappings for a Chat Completions upstream.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OpenAiCompatibilityGroup {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub models: Vec<ModelMapping>,
    #[serde(deserialize_with = "deserialize_group_entries")]
    pub keys: Vec<OpenAiCompatibilityApiKey>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct OpenAiCompatibilityApiKey {
    #[serde(default)]
    pub api_key: String,
    pub proxy_url: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ModelMapping {
    pub name: String,
    pub alias: String,
}

fn deserialize_group_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // The YAML deserializer otherwise reads null as an empty list or mapping.
    let entries = Option::<Vec<Option<T>>>::deserialize(deserializer)?
        .ok_or_else(|| D::Error::custom("expected a list"))?;
    entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.ok_or_else(|| D::Error::custom(format!("expected a mapping at index {index}")))
        })
        .collect()
}
