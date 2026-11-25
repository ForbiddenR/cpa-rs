//! Model listing, Chat Completions, and Responses endpoints.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::header::{CONNECTION, HeaderName, HeaderValue, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json, value::RawValue};

use super::auth::allows_bearer;
use crate::app::AppState;
use crate::proxy::upstream::UpstreamEndpoint;
use crate::routing::{RouteError, list_codex_models, resolve_codex};

pub(crate) async fn models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, OpenAiError> {
    if !allows_bearer(&state.config.access, &headers) {
        return Err(OpenAiError::Unauthorized);
    }

    let data: Vec<_> = list_codex_models(&state.config)
        .into_iter()
        .map(|id| json!({"id": id, "object": "model", "created": 0, "owned_by": "cpa-rs"}))
        .collect();
    Ok(Json(json!({"object": "list", "data": data})))
}

pub(crate) async fn responses(
    State(state): State<Arc<AppState>>,
    request: Request,
) -> Result<Response, OpenAiError> {
    if !allows_bearer(&state.config.access, request.headers()) {
        return Err(OpenAiError::Unauthorized);
    }

    let (route, stream, body) = {
        let input = to_bytes(request.into_body(), usize::MAX)
            .await
            .map_err(|_| OpenAiError::InvalidBody("Could not read the request body"))?;
        // Keep the JSON values of all fields other than model intact.
        let mut payload: BTreeMap<String, &RawValue> = serde_json::from_slice(&input)
            .map_err(|_| OpenAiError::InvalidBody("Expected a JSON object in the request body"))?;
        let model: String = payload
            .get("model")
            .and_then(|value| serde_json::from_str(value.get()).ok())
            .ok_or(OpenAiError::InvalidModel)?;
        let route = resolve_codex(&state.config, &model).map_err(OpenAiError::Route)?;
        let stream = payload
            .get("stream")
            .is_some_and(|value| value.get() == "true");
        let model = serde_json::value::to_raw_value(route.upstream_model)
            .map_err(|_| OpenAiError::Encode)?;
        payload.insert("model".to_owned(), &model);
        let body = serde_json::to_vec(&payload).map_err(|_| OpenAiError::Encode)?;
        (route, stream, body)
    };

    let mut upstream = state
        .upstream(route.proxy_url)
        .post_json(
            route.base_url,
            UpstreamEndpoint::Responses,
            route.api_key,
            body,
            stream,
        )
        .await
        .map_err(|_| OpenAiError::Upstream)?;
    let status = upstream.status();
    let mut headers = std::mem::take(upstream.headers_mut());
    strip_hop_by_hop_headers(&mut headers);
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

fn strip_hop_by_hop_headers(headers: &mut HeaderMap) {
    let connection_headers: Vec<_> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect();
    for name in connection_headers {
        headers.remove(name);
    }
    for name in [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "proxy-connection",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ] {
        headers.remove(name);
    }
}

pub(crate) enum OpenAiError {
    Unauthorized,
    InvalidBody(&'static str),
    InvalidModel,
    Route(RouteError),
    Encode,
    Upstream,
}

impl IntoResponse for OpenAiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "authentication_error",
                "Missing or invalid API key",
            ),
            Self::InvalidBody(message) => {
                (StatusCode::BAD_REQUEST, "invalid_request_error", message)
            }
            Self::InvalidModel | Self::Route(RouteError::EmptyModel) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "model must be a non-empty string",
            ),
            Self::Route(RouteError::ModelNotFound { .. }) => (
                StatusCode::NOT_FOUND,
                "invalid_request_error",
                "Model is not configured",
            ),
            Self::Route(RouteError::NoCredentials { .. }) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "No upstream credentials are available for this model",
            ),
            Self::Encode => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Could not encode the upstream request",
            ),
            Self::Upstream => (
                StatusCode::BAD_GATEWAY,
                "server_error",
                "Upstream request failed",
            ),
        };
        let mut response = (
            status,
            Json(json!({"error": {"type": kind, "message": message}})),
        )
            .into_response();
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}
