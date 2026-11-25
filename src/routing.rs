//! Model alias resolution and upstream selection.

use thiserror::Error;

use crate::config::{Config, ModelMapping};

/// The upstream settings for a native Responses request.
pub struct CodexRoute<'a> {
    pub base_url: &'a str,
    pub upstream_model: &'a str,
    /// An empty value allows a configured upstream without bearer authentication.
    pub api_key: &'a str,
    pub proxy_url: Option<&'a str>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RouteError {
    #[error("model must not be empty")]
    EmptyModel,
    #[error("unknown Codex model {model:?}")]
    ModelNotFound { model: String },
    #[error("no eligible Codex credentials for model {model:?}")]
    NoCredentials { model: String },
}

/// Resolves a configured model to the first eligible Codex key in YAML order.
pub fn resolve_codex<'a>(
    config: &'a Config,
    requested_model: &str,
) -> Result<CodexRoute<'a>, RouteError> {
    let requested_model = requested_model.trim();
    if requested_model.is_empty() {
        return Err(RouteError::EmptyModel);
    }

    let mut declared_by_group = false;
    for group in &config.api_keys.codex {
        let group_model = match_model(&group.models, requested_model);
        declared_by_group |= group_model.is_some();

        for key in &group.keys {
            let upstream_model = match &key.models {
                Some(models) => match_model(models, requested_model),
                None => group_model,
            };
            if let Some(upstream_model) = upstream_model {
                return Ok(CodexRoute {
                    base_url: &group.base_url,
                    upstream_model,
                    api_key: &key.api_key,
                    proxy_url: key.proxy_url.as_deref(),
                });
            }
        }
    }

    let model = requested_model.to_owned();
    if declared_by_group {
        Err(RouteError::NoCredentials { model })
    } else {
        Err(RouteError::ModelNotFound { model })
    }
}

fn match_model<'a>(models: &'a [ModelMapping], requested_model: &str) -> Option<&'a str> {
    models.iter().find_map(|model| {
        let name = model.name.trim();
        let alias = model.alias.trim();
        let client_model = if alias.is_empty() { name } else { alias };
        let upstream_model = if name.is_empty() { client_model } else { name };
        (client_model == requested_model).then_some(upstream_model)
    })
}
