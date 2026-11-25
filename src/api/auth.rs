//! Authentication for inference clients and management requests.

use axum::http::{HeaderMap, header::AUTHORIZATION};

use crate::config::AccessConfig;

pub(super) fn allows_bearer(access: &AccessConfig, headers: &HeaderMap) -> bool {
    let mut keys = access
        .api_keys
        .iter()
        .map(|key| key.trim())
        .filter(|key| !key.is_empty())
        .peekable();
    if keys.peek().is_none() {
        return true;
    }

    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim());
    bearer.is_some_and(|token| keys.any(|key| key == token))
}
