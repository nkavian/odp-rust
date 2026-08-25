use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequest {
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
    pub method: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpResponse {
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
    pub status: u16,
}

#[derive(Debug, Error)]
#[error("HTTP transport failed: {message}")]
pub struct TransportError {
    pub message: String,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError>;
}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| TransportError {
                message: error.to_string(),
            })?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|error| {
            TransportError {
                message: error.to_string(),
            }
        })?;
        let mut builder = self.client.request(method, request.url).body(request.body);
        for (name, value) in request.headers {
            builder = builder.header(&name, &value);
        }
        let response = builder.send().await.map_err(|error| TransportError {
            message: error.to_string(),
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_ascii_lowercase(), value.to_owned()))
            })
            .collect();
        let body = response.bytes().await.map_err(|error| TransportError {
            message: error.to_string(),
        })?;
        Ok(HttpResponse {
            body: body.to_vec(),
            headers,
            status,
        })
    }
}

pub(crate) fn default_transport() -> Result<Arc<dyn Transport>, TransportError> {
    Ok(Arc::new(ReqwestTransport::new()?))
}
