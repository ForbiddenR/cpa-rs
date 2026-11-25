//! Model alias resolution and upstream selection.

use std::collections::HashSet;

use thiserror::Error;

use crate::config::{CodexApiKey, CodexGroup, Config, ModelMapping};

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

    for group in &config.api_keys.codex {
        for (key, models) in codex_model_sources(group) {
            if let Some(upstream_model) = match_model(models, requested_model) {
                return Ok(CodexRoute {
                    base_url: &group.base_url,
                    upstream_model,
                    api_key: &key.api_key,
                    proxy_url: key.proxy_url.as_deref(),
                });
            }
        }
    }

    let declared_by_group = config
        .api_keys
        .codex
        .iter()
        .any(|group| match_model(&group.models, requested_model).is_some());
    let model = requested_model.to_owned();
    if declared_by_group {
        Err(RouteError::NoCredentials { model })
    } else {
        Err(RouteError::ModelNotFound { model })
    }
}

/// Lists unique model IDs in the order they become routable in the configuration.
pub fn list_codex_models(config: &Config) -> Vec<&str> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for (_, models) in config.api_keys.codex.iter().flat_map(codex_model_sources) {
        for id in models.iter().map(client_model) {
            if !id.is_empty() && seen.insert(id) {
                ids.push(id);
            }
        }
    }
    ids
}

fn codex_model_sources(
    group: &CodexGroup,
) -> impl Iterator<Item = (&CodexApiKey, &[ModelMapping])> {
    // The first key inheriting the group models takes precedence over later inheriting keys.
    let mut includes_group_models = false;
    group.keys.iter().filter_map(move |key| {
        let models = match key.models.as_deref() {
            Some(models) => models,
            None if !includes_group_models => {
                includes_group_models = true;
                &group.models
            }
            None => return None,
        };
        Some((key, models))
    })
}

fn client_model(model: &ModelMapping) -> &str {
    let alias = model.alias.trim();
    if alias.is_empty() {
        model.name.trim()
    } else {
        alias
    }
}

fn match_model<'a>(models: &'a [ModelMapping], requested_model: &str) -> Option<&'a str> {
    models.iter().find_map(|model| {
        let client_model = client_model(model);
        let name = model.name.trim();
        let upstream_model = if name.is_empty() { client_model } else { name };
        (client_model == requested_model).then_some(upstream_model)
    })
}
