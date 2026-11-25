//! Integration tests for the upstream HTTP transport.

use std::time::Duration;

use cpa_rs::proxy::upstream::{UpstreamClient, UpstreamEndpoint};
use reqwest::StatusCode;
use reqwest::header::{CONTENT_TYPE, LOCATION, RETRY_AFTER};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn responses_stream_arrives_before_upstream_finishes() {
    const PAYLOAD: &str = r#"{"model":"responses-model","stream":true}"#;
    const FIRST_EVENT: &str = "event: response.created\ndata: {}\n\n";
    const LAST_EVENT: &str = "event: response.completed\ndata: {}\n\n";

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}/tenant/v1/", listener.local_addr().unwrap());
    let (finish_tx, finish_rx) = oneshot::channel();
    let upstream = tokio::spawn(async move {
        let (mut socket, headers) = read_request(&listener, PAYLOAD).await;
        assert!(headers.starts_with("POST /tenant/v1/responses HTTP/1.1\r\n"));
        assert!(has_header(&headers, "content-type: application/json"));
        assert!(has_header(&headers, "accept: text/event-stream"));
        assert!(has_header(&headers, "authorization: Bearer upstream-key"));

        let length = FIRST_EVENT.len() + LAST_EVENT.len();
        let initial = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
             Content-Length: {length}\r\nConnection: close\r\n\r\n{FIRST_EVENT}"
        );
        socket.write_all(initial.as_bytes()).await.unwrap();
        finish_rx.await.unwrap();
        socket.write_all(LAST_EVENT.as_bytes()).await.unwrap();
    });

    let client = UpstreamClient::new(Some("direct")).unwrap();
    let mut response = timeout(
        TIMEOUT,
        client.post_json(
            &base_url,
            UpstreamEndpoint::Responses,
            "upstream-key",
            PAYLOAD,
            true,
        ),
    )
    .await
    .expect("upstream response headers did not arrive")
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "text/event-stream");

    let first_event = timeout(TIMEOUT, async {
        let mut bytes = Vec::new();
        while bytes.len() < FIRST_EVENT.len() {
            bytes.extend_from_slice(&response.chunk().await.unwrap().unwrap());
        }
        bytes
    })
    .await
    .expect("upstream SSE data did not arrive before the response ended");
    assert_eq!(first_event, FIRST_EVENT.as_bytes());

    finish_tx.send(()).unwrap();
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        LAST_EVENT.as_bytes()
    );
    upstream.await.unwrap();
}

#[tokio::test]
async fn chat_completions_preserves_error_response_through_configured_proxy() {
    const PAYLOAD: &str = r#"{"model":"chat-model","messages":[]}"#;
    const ERROR_BODY: &str = r#"{"error":{"message":"rate limited"}}"#;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move {
        let (mut socket, headers) = read_request(&listener, PAYLOAD).await;
        assert!(
            headers
                .starts_with("POST http://chat.example.invalid/v1/chat/completions HTTP/1.1\r\n")
        );
        assert!(has_header(&headers, "accept: application/json"));
        assert!(!headers.to_ascii_lowercase().contains("\r\nauthorization:"));

        let length = ERROR_BODY.len();
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\n\
             Retry-After: 7\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{ERROR_BODY}"
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let client = UpstreamClient::new(Some(&proxy_url)).unwrap();
    let response = timeout(
        TIMEOUT,
        client.post_json(
            "http://chat.example.invalid/v1",
            UpstreamEndpoint::ChatCompletions,
            "",
            PAYLOAD,
            false,
        ),
    )
    .await
    .expect("upstream error response did not arrive")
    .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/json");
    assert_eq!(response.headers()[RETRY_AFTER], "7");
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        ERROR_BODY.as_bytes()
    );
    upstream.await.unwrap();
}

#[tokio::test]
async fn returns_redirect_status_to_the_caller() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let upstream = tokio::spawn(async move {
        let (mut socket, headers) = read_request(&listener, "{}").await;
        assert!(headers.starts_with("POST /responses HTTP/1.1\r\n"));
        socket
            .write_all(
                b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /other\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
    });

    let client = UpstreamClient::new(Some("none")).unwrap();
    let response = timeout(
        TIMEOUT,
        client.post_json(&base_url, UpstreamEndpoint::Responses, "", "{}", false),
    )
    .await
    .expect("upstream redirect response did not arrive")
    .unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers()[LOCATION], "/other");
    upstream.await.unwrap();
}

async fn read_request(listener: &TcpListener, body: &str) -> (TcpStream, String) {
    let (socket, _) = listener.accept().await.unwrap();
    let mut reader = BufReader::new(socket);
    let mut headers = String::new();
    let mut line = String::new();

    loop {
        let size = reader.read_line(&mut line).await.unwrap();
        assert_ne!(size, 0, "request ended before all headers arrived");
        if line == "\r\n" {
            break;
        }
        headers.push_str(&line);
        line.clear();
    }

    let mut received = vec![0; body.len()];
    reader.read_exact(&mut received).await.unwrap();
    assert_eq!(received, body.as_bytes());
    (reader.into_inner(), headers)
}

fn has_header(headers: &str, expected: &str) -> bool {
    headers
        .lines()
        .any(|header| header.eq_ignore_ascii_case(expected))
}
