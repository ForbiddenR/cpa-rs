//! Checks configuration values after YAML deserialization.

use url::Url;
use uuid::Uuid;

use super::{CONFIG_VERSION, Config, ConfigError, ModelMapping};

impl Config {
    /// Checks the supported configuration fields before use.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.config_version != CONFIG_VERSION {
            return Err(invalid_field("config-version", "expected version 8"));
        }

        let id = self.server.server_id.trim();
        if !id.is_empty()
            && (id.len() != 36 || !Uuid::parse_str(id).is_ok_and(|uuid| !uuid.is_nil()))
        {
            return Err(invalid_field(
                "server.server-id",
                "expected a nonzero hyphenated UUID",
            ));
        }

        for (index, group) in self.api_keys.codex.iter().enumerate() {
            let path = format!("api-keys.codex[{index}]");
            validate_base_url(&group.base_url, &path, !group.keys.is_empty())?;
            validate_models(&group.models, &path)?;

            for (index, key) in group.keys.iter().enumerate() {
                let path = format!("{path}.keys[{index}]");
                validate_proxy_url(key.proxy_url.as_deref(), &path)?;
                if let Some(models) = &key.models {
                    validate_models(models, &path)?;
                }
            }
        }

        for (index, group) in self.api_keys.openai_compatibility.iter().enumerate() {
            let path = format!("api-keys.openai-compatibility[{index}]");
            validate_base_url(&group.base_url, &path, !group.disabled)?;
            validate_models(&group.models, &path)?;

            for (index, key) in group.keys.iter().enumerate() {
                validate_proxy_url(key.proxy_url.as_deref(), &format!("{path}.keys[{index}]"))?;
            }
        }
        Ok(())
    }
}

fn validate_base_url(value: &str, path: &str, active: bool) -> Result<(), ConfigError> {
    if value.trim().is_empty() && !active {
        return Ok(());
    }

    if parse_base_url(value).is_none() {
        return Err(invalid_field(
            format!("{path}.base-url"),
            "expected an HTTP or HTTPS URL with a host and no query or fragment",
        ));
    }
    Ok(())
}

fn validate_proxy_url(value: Option<&str>, path: &str) -> Result<(), ConfigError> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("direct")
        || value.eq_ignore_ascii_case("none")
    {
        return Ok(());
    }

    if parse_proxy_url(value).is_none() {
        return Err(invalid_field(
            format!("{path}.proxy-url"),
            "expected an HTTP, HTTPS, SOCKS5, or SOCKS5H URL, direct, or none",
        ));
    }
    Ok(())
}

pub(crate) fn parse_base_url(value: &str) -> Option<Url> {
    parse_endpoint(value).filter(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

pub(crate) fn parse_proxy_url(value: &str) -> Option<Url> {
    parse_endpoint(value)
        .filter(|url| matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h"))
}

fn parse_endpoint(value: &str) -> Option<Url> {
    let value = value.trim();
    let (_, authority) = value.split_once("://")?;
    if authority.starts_with(['/', '\\']) {
        return None;
    }
    Url::parse(value).ok().filter(Url::has_host)
}

fn validate_models(models: &[ModelMapping], path: &str) -> Result<(), ConfigError> {
    for (index, model) in models.iter().enumerate() {
        if model.name.trim().is_empty() && model.alias.trim().is_empty() {
            return Err(invalid_field(
                format!("{path}.models[{index}]"),
                "expected a non-empty name or alias",
            ));
        }
    }
    Ok(())
}

fn invalid_field(field: impl Into<String>, message: &'static str) -> ConfigError {
    ConfigError::InvalidField {
        field: field.into(),
        message,
    }
}
