//! YAML configuration loading.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;
use yaml_serde::Value;

use super::Config;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read config file {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid YAML: {0}")]
    Yaml(#[from] yaml_serde::Error),
    #[error("invalid configuration: {0}")]
    Decode(#[from] serde_path_to_error::Error<yaml_serde::Error>),
    #[error("{field}: {message}")]
    InvalidField {
        field: String,
        message: &'static str,
    },
}

impl Config {
    /// Reads and validates the YAML configuration at the given path.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_yaml(&input)
    }

    /// Parses and validates a single CPA v8 YAML document.
    pub fn from_yaml(input: &str) -> Result<Self, ConfigError> {
        let mut document: Value = yaml_serde::from_str(input)?;
        if !document.is_mapping() {
            return Err(ConfigError::InvalidField {
                field: "config".into(),
                message: "expected a YAML mapping",
            });
        }

        document.apply_merge()?;
        let config: Self = serde_path_to_error::deserialize(document)?;
        config.validate()?;
        Ok(config)
    }
}
