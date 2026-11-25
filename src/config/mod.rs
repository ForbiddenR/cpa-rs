//! Configuration types for the supported CLIProxyAPI v8 fields.

mod store;

#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub config_version: u32,
    pub server: ServerConfig,
    pub management: ManagementConfig,
    pub access: AccessConfig,
    pub api_keys: UpstreamApiKeys,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub server_id: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ManagementConfig {
    pub allow_remote: bool,
    pub secret_key: String,
    pub disable_control_panel: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct AccessConfig {
    /// Client keys used to authenticate requests to this proxy.
    pub api_keys: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct UpstreamApiKeys {
    pub codex: Vec<CodexGroup>,
    pub openai_compatibility: Vec<OpenAiCompatibilityGroup>,
}

/// Credentials and model mappings for a Responses upstream.
#[derive(Clone, PartialEq, Eq)]
pub struct CodexGroup {
    pub name: String,
    pub base_url: String,
    pub models: Vec<ModelMapping>,
    pub keys: Vec<CodexApiKey>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CodexApiKey {
    pub api_key: String,
    pub proxy_url: Option<String>,
    /// An omitted list inherits the group mappings. An empty list overrides them.
    pub models: Option<Vec<ModelMapping>>,
}

/// Credentials and model mappings for a Chat Completions upstream.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiCompatibilityGroup {
    pub name: String,
    pub disabled: bool,
    pub base_url: String,
    pub models: Vec<ModelMapping>,
    pub keys: Vec<OpenAiCompatibilityApiKey>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct OpenAiCompatibilityApiKey {
    pub api_key: String,
    pub proxy_url: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ModelMapping {
    pub name: String,
    pub alias: String,
}
