//! Integration tests for the OpenAI endpoints.

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use cpa_rs::routing::resolve_codex;
use cpa_rs::{app, config::Config};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn models_requires_client_auth_and_lists_only_routable_model_ids() {
    let config = Config::from_yaml(
        r#"
access:
  api-keys: [client-key]
api-keys:
  codex:
    - models: [{alias: no-credentials}]
      keys: []
    - base-url: http://unused.invalid/v1
      models: [{alias: overridden}]
      keys: [{api-key: upstream-key, proxy-url: direct, models: []}]
    - base-url: http://unused.invalid/v1
      models:
        - {name: shared-model, alias: shared}
        - {name: ' inherited '}
      keys:
        - api-key: upstream-key
          proxy-url: direct
          models:
            - {name: private-name, alias: ' shared '}
            - {alias: alias-only}
        - proxy-url: direct
        - proxy-url: direct
  openai-compatibility:
    - base-url: http://unused.invalid/v1
      models: [{alias: chat-only}]
      keys: [{api-key: upstream-key}]
"#,
    )
    .unwrap();
    let base_url = serve(app::router(config.clone()).unwrap()).await;
    let url = format!("{base_url}/v1/models");
    let client = client();

    let rejected = client.get(&url).send().await.unwrap();
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(rejected.headers()["www-authenticate"], "Bearer");

    let response = client
        .get(&url)
        .bearer_auth("client-key")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    let payload: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(payload["object"], "list");
    let models = payload["data"].as_array().unwrap();
    let ids: Vec<_> = models
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["shared", "alias-only", "inherited"]);
    assert_eq!(
        models[0],
        json!({"id": "shared", "object": "model", "created": 0, "owned_by": "cpa-rs"})
    );
    for id in ids {
        assert!(resolve_codex(&config, id).is_ok(), "unroutable model {id}");
    }
}

#[tokio::test]
async fn models_returns_an_empty_list_without_routes() {
    let base_url = serve(app::router(Config::default()).unwrap()).await;
    let response = client()
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
    assert_eq!(payload, json!({"object": "list", "data": []}));
}

#[tokio::test]
async fn responses_authenticates_rewrites_model_and_streams_incrementally() {
    const FIRST: &str = "event: response.created\ndata: {}\n\n";
    const LAST: &str = "event: response.completed\ndata: {}\n\n";
    let (finish, wait) = oneshot::channel();
    let wait = Arc::new(Mutex::new(Some(wait)));
    let upstream = serve(Router::new().route(
        "/v1/responses",
        post(
            move |headers: HeaderMap, Json(payload): Json<Value>| async move {
                assert_eq!(headers["authorization"], "Bearer upstream-key");
                assert_eq!(headers["accept"], "text/event-stream");
                assert_eq!(
                    payload,
                    json!({
                        "model": "actual-model",
                        "input": "Hello",
                        "stream": true,
                        "metadata": {"source": "test"},
                    })
                );
                let wait = wait.lock().unwrap().take().unwrap();
                let first =
                    stream::iter([Ok::<_, Infallible>(Bytes::from_static(FIRST.as_bytes()))]);
                let last = stream::once(async move {
                    wait.await.unwrap();
                    Ok(Bytes::from_static(LAST.as_bytes()))
                });
                (
                    [("content-type", "text/event-stream")],
                    Body::from_stream(first.chain(last)),
                )
            },
        ),
    ))
    .await;
    let proxy = proxy(&upstream, "[' client-key ']").await;
    let client = client();

    let rejected = client
        .post(&proxy)
        .bearer_auth("upstream-key")
        .body("invalid JSON")
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(rejected.headers()["www-authenticate"], "Bearer");

    let mut response = client
        .post(&proxy)
        .bearer_auth("client-key")
        .body(
            r#"{"model":"codex-test","input":"Hello","stream":true,"metadata":{"source":"test"}}"#,
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let mut first = Vec::new();
    while first.len() < FIRST.len() {
        first.extend_from_slice(&response.chunk().await.unwrap().unwrap());
    }
    assert_eq!(first, FIRST.as_bytes());
    finish.send(()).unwrap();
    assert_eq!(response.text().await.unwrap(), LAST);
}

#[tokio::test]
async fn responses_preserves_upstream_error_body_status_and_end_to_end_headers() {
    const ERROR: &str = r#"{"error":{"message":"rate limited"}}"#;
    let upstream = serve(Router::new().route(
        "/v1/responses",
        post(|headers: HeaderMap| async move {
            assert_eq!(headers["accept"], "application/json");
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("content-type", "application/json"),
                    ("retry-after", "7"),
                    ("connection", "x-hop-only"),
                    ("x-hop-only", "upstream connection only"),
                ],
                ERROR,
            )
        }),
    ))
    .await;
    let response = client()
        .post(proxy(&upstream, "[]").await)
        .body(r#"{"model":"codex-test","input":"Hello"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["retry-after"], "7");
    assert!(!response.headers().contains_key("x-hop-only"));
    assert_eq!(response.text().await.unwrap(), ERROR);
}

#[tokio::test]
async fn responses_reports_invalid_requests_and_unknown_models_as_json() {
    let proxy = proxy("http://unused.invalid", "[]").await;
    let client = client();
    for (body, status) in [
        ("[]", StatusCode::BAD_REQUEST),
        (r#"{"model":123}"#, StatusCode::BAD_REQUEST),
        (r#"{"model":" "}"#, StatusCode::BAD_REQUEST),
        (r#"{"model":"missing"}"#, StatusCode::NOT_FOUND),
    ] {
        let response = client.post(&proxy).body(body).send().await.unwrap();
        assert_eq!(response.status(), status);
        let payload: Value = serde_json::from_str(&response.text().await.unwrap()).unwrap();
        assert_eq!(payload["error"]["type"], "invalid_request_error");
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(TIMEOUT)
        .build()
        .unwrap()
}

async fn proxy(upstream: &str, client_keys: &str) -> String {
    let config = Config::from_yaml(&format!(
        "\
access:
  api-keys: {client_keys}
api-keys:
  codex:
    - base-url: {upstream}/v1
      models: [{{name: actual-model, alias: codex-test}}]
      keys: [{{api-key: upstream-key, proxy-url: direct}}]
"
    ))
    .unwrap();
    format!("{}/v1/responses", serve(app::router(config).unwrap()).await)
}

async fn serve(router: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}
