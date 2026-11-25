//! HTTP transport for OpenAI Responses and Chat Completions upstreams.

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest::{Body, Client, Proxy, Response};
use thiserror::Error;

use crate::config::{parse_base_url, parse_proxy_url};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpstreamEndpoint {
    Responses,
    ChatCompletions,
}

impl UpstreamEndpoint {
    fn path_segments(self) -> &'static [&'static str] {
        match self {
            Self::Responses => &["responses"],
            Self::ChatCompletions => &["chat", "completions"],
        }
    }
}

#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("invalid upstream base URL")]
    InvalidBaseUrl,
    #[error("invalid upstream proxy URL")]
    InvalidProxyUrl,
    #[error("invalid upstream authorization header")]
    InvalidAuthorization,
    #[error("could not initialize the upstream HTTP client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("upstream HTTP request failed: {0}")]
    Request(#[source] reqwest::Error),
}

/// A reusable HTTP client for upstreams that share a proxy setting.
#[derive(Clone)]
pub struct UpstreamClient {
    client: Client,
}

impl UpstreamClient {
    /// An absent or blank proxy setting inherits the environment's proxy configuration.
    pub fn new(proxy_url: Option<&str>) -> Result<Self, UpstreamError> {
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());

        if let Some(value) = proxy_url.map(str::trim).filter(|value| !value.is_empty()) {
            if value.eq_ignore_ascii_case("direct") || value.eq_ignore_ascii_case("none") {
                builder = builder.no_proxy();
            } else {
                let url = parse_proxy_url(value).ok_or(UpstreamError::InvalidProxyUrl)?;
                let proxy = Proxy::all(url).map_err(|_| UpstreamError::InvalidProxyUrl)?;
                builder = builder.proxy(proxy);
            }
        }

        let client = builder
            .build()
            .map_err(|error| UpstreamError::Client(error.without_url()))?;
        Ok(Self { client })
    }

    /// Sends a JSON payload and returns the response as soon as its headers arrive.
    /// The caller can read it incrementally, including for upstream error statuses.
    pub async fn post_json(
        &self,
        base_url: &str,
        endpoint: UpstreamEndpoint,
        api_key: &str,
        body: impl Into<Body>,
        stream: bool,
    ) -> Result<Response, UpstreamError> {
        let mut url = parse_base_url(base_url).ok_or(UpstreamError::InvalidBaseUrl)?;
        url.path_segments_mut()
            .map_err(|_| UpstreamError::InvalidBaseUrl)?
            .pop_if_empty()
            .extend(endpoint.path_segments());

        let accept = if stream {
            "text/event-stream"
        } else {
            "application/json"
        };
        let mut request = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, accept)
            .body(body)
            .build()
            .map_err(|error| UpstreamError::Request(error.without_url()))?;

        let api_key = api_key.trim();
        if !api_key.is_empty() {
            let mut authorization = HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|_| UpstreamError::InvalidAuthorization)?;
            authorization.set_sensitive(true);
            request.headers_mut().insert(AUTHORIZATION, authorization);
        }

        self.client
            .execute(request)
            .await
            .map_err(|error| UpstreamError::Request(error.without_url()))
    }
}
