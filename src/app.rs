//! Router construction and application state.

use std::collections::{HashMap, hash_map::Entry};
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};

use crate::api::openai;
use crate::config::Config;
use crate::proxy::upstream::{UpstreamClient, UpstreamError};

pub(crate) struct AppState {
    pub(crate) config: Config,
    upstreams: HashMap<String, UpstreamClient>,
}

impl AppState {
    pub(crate) fn upstream(&self, proxy_url: Option<&str>) -> &UpstreamClient {
        // The router initializes a client for every proxy setting the resolver can return.
        &self.upstreams[proxy_url.unwrap_or_default().trim()]
    }
}

/// Builds the HTTP router from a validated configuration.
pub fn router(config: Config) -> Result<Router, UpstreamError> {
    let mut upstreams = HashMap::new();
    for group in &config.api_keys.codex {
        for key in &group.keys {
            let proxy_url = key.proxy_url.as_deref().unwrap_or_default().trim();
            if let Entry::Vacant(entry) = upstreams.entry(proxy_url.to_owned()) {
                entry.insert(UpstreamClient::new(Some(proxy_url))?);
            }
        }
    }

    let state = Arc::new(AppState { config, upstreams });
    Ok(Router::new()
        .route("/v1/models", get(openai::models))
        .route("/v1/responses", post(openai::responses))
        .with_state(state))
}
