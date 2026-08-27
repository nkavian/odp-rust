use std::{collections::BTreeMap, sync::Arc};

use odp_core::derive_service_origin;
use thiserror::Error;
use url::Url;

use crate::{
    DirectoryService, Environment, HttpRequest, HttpResponse, IterationOptions, SearchPage,
    SearchRequest, SuggestionRequest, Transport, TransportError, default_transport,
};

const MAXIMUM_REDIRECTS: usize = 5;
const MAXIMUM_RESPONSE_BYTES: usize = 524_288;

#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("invalid Directory request: {0}")]
    InvalidRequest(String),
    #[error("invalid Directory response: {0}")]
    InvalidResponse(String),
    #[error("Directory request failed with HTTP {status}: {message}")]
    Request {
        headers: BTreeMap<String, String>,
        message: String,
        status: u16,
    },
}

#[derive(Clone)]
pub struct DirectoryClient {
    environment: Environment,
    transport: Arc<dyn Transport>,
}

impl DirectoryClient {
    pub fn new(environment: Environment) -> Result<Self, DirectoryError> {
        Ok(Self {
            environment,
            transport: default_transport()?,
        })
    }

    pub fn with_transport(environment: Environment, transport: Arc<dyn Transport>) -> Self {
        Self {
            environment,
            transport,
        }
    }

    pub const fn environment(&self) -> Environment {
        self.environment
    }

    pub async fn search(&self, request: &SearchRequest) -> Result<SearchPage, DirectoryError> {
        validate_search_request(request)?;
        let body = serde_json::to_vec(request)
            .map_err(|error| DirectoryError::InvalidRequest(error.to_string()))?;
        self.request_page(
            "POST",
            &format!("{}/v1/services/search", self.environment.origin()),
            body,
        )
        .await
    }

    pub async fn continue_search(&self, next: &str) -> Result<SearchPage, DirectoryError> {
        let target = self.continuation_url(next)?;
        self.request_page("GET", target.as_str(), Vec::new()).await
    }

    pub async fn search_pages(
        &self,
        request: &SearchRequest,
        options: IterationOptions,
    ) -> Result<Vec<SearchPage>, DirectoryError> {
        let maximum_pages = bounded(options.max_pages, 16, 16, "max_pages")?;
        let mut pages = Vec::new();
        let mut page = self.search(request).await?;
        for _ in 0..maximum_pages {
            let next = page.next.clone();
            pages.push(page);
            if next.is_empty() {
                return Ok(pages);
            }
            page = self.continue_search(&next).await?;
        }
        Ok(pages)
    }

    pub async fn search_services(
        &self,
        request: &SearchRequest,
        options: IterationOptions,
    ) -> Result<Vec<DirectoryService>, DirectoryError> {
        let maximum_items = bounded(options.max_items, 10_000, 10_000, "max_items")?;
        let pages = self.search_pages(request, options).await?;
        Ok(pages
            .into_iter()
            .flat_map(|page| page.items)
            .take(maximum_items)
            .collect())
    }

    pub async fn suggest(
        &self,
        request: &SuggestionRequest,
    ) -> Result<Vec<String>, DirectoryError> {
        let prefix = request.prefix.trim();
        if prefix.is_empty() || prefix.chars().count() > 128 {
            return Err(DirectoryError::InvalidRequest(
                "prefix must contain from 1 through 128 characters".to_owned(),
            ));
        }
        if request.limit > 25 {
            return Err(DirectoryError::InvalidRequest(
                "limit must be from 1 through 25".to_owned(),
            ));
        }
        let mut target = Url::parse(&format!(
            "{}/v1/services/suggestions",
            self.environment.origin()
        ))
        .map_err(|error| DirectoryError::InvalidRequest(error.to_string()))?;
        target.query_pairs_mut().append_pair("prefix", prefix);
        if request.limit != 0 {
            target
                .query_pairs_mut()
                .append_pair("limit", &request.limit.to_string());
        }
        let response = self.request("GET", target, Vec::new()).await?;
        let suggestions = serde_json::from_slice::<Vec<String>>(&response.body)
            .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
        if suggestions.len() > 25
            || suggestions.iter().any(|value| {
                value.trim() != value || value.is_empty() || value.chars().count() > 128
            })
        {
            return Err(DirectoryError::InvalidResponse(
                "Directory suggestions are invalid".to_owned(),
            ));
        }
        Ok(suggestions)
    }

    async fn request_page(
        &self,
        method: &str,
        target: &str,
        body: Vec<u8>,
    ) -> Result<SearchPage, DirectoryError> {
        let target = Url::parse(target)
            .map_err(|error| DirectoryError::InvalidRequest(error.to_string()))?;
        let response = self.request(method, target, body).await?;
        let page = serde_json::from_slice::<SearchPage>(&response.body)
            .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
        if page.items.len() > 100 {
            return Err(DirectoryError::InvalidResponse(
                "Directory search page exceeds 100 Services".to_owned(),
            ));
        }
        for service in &page.items {
            let canonical = derive_service_origin(&service.service_origin)
                .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
            if canonical != service.service_origin {
                return Err(DirectoryError::InvalidResponse(
                    "Directory Service origin is not canonical".to_owned(),
                ));
            }
        }
        Ok(page)
    }

    async fn request(
        &self,
        mut method: &str,
        mut target: Url,
        mut body: Vec<u8>,
    ) -> Result<HttpResponse, DirectoryError> {
        for redirects in 0..=MAXIMUM_REDIRECTS {
            let mut headers =
                BTreeMap::from([("accept".to_owned(), "application/json".to_owned())]);
            if !body.is_empty() {
                headers.insert("content-type".to_owned(), "application/json".to_owned());
            }
            let response = self
                .transport
                .send(HttpRequest {
                    body: body.clone(),
                    headers,
                    method: method.to_owned(),
                    url: target.to_string(),
                })
                .await?;
            if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                return consume_response(response);
            }
            if redirects == MAXIMUM_REDIRECTS {
                return Err(DirectoryError::InvalidResponse(
                    "Directory response exceeded five redirects".to_owned(),
                ));
            }
            let location = response.headers.get("location").ok_or_else(|| {
                DirectoryError::InvalidResponse("Directory redirect omitted Location".to_owned())
            })?;
            let next = target
                .join(location)
                .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
            if next.origin() != target.origin() {
                return Err(DirectoryError::InvalidResponse(
                    "Directory redirect changed origin".to_owned(),
                ));
            }
            if response.status == 303 || (matches!(response.status, 301 | 302) && method == "POST")
            {
                method = "GET";
                body.clear();
            }
            target = next;
        }
        Err(DirectoryError::InvalidResponse(
            "Directory response exceeded its redirect limit".to_owned(),
        ))
    }

    fn continuation_url(&self, next: &str) -> Result<Url, DirectoryError> {
        let origin = Url::parse(self.environment.origin())
            .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
        let target = origin
            .join(next)
            .map_err(|error| DirectoryError::InvalidResponse(error.to_string()))?;
        if target.origin() != origin.origin()
            || !target.username().is_empty()
            || target.password().is_some()
        {
            return Err(DirectoryError::InvalidResponse(
                "Directory continuation changed canonical origin".to_owned(),
            ));
        }
        Ok(target)
    }
}

fn bounded(
    value: usize,
    fallback: usize,
    maximum: usize,
    name: &str,
) -> Result<usize, DirectoryError> {
    let value = if value == 0 { fallback } else { value };
    if value > maximum {
        return Err(DirectoryError::InvalidRequest(format!(
            "{name} must be from 1 through {maximum}"
        )));
    }
    Ok(value)
}

fn validate_search_request(request: &SearchRequest) -> Result<(), DirectoryError> {
    if request.limit > 100 {
        return Err(DirectoryError::InvalidRequest(
            "limit must be from 1 through 100".to_owned(),
        ));
    }
    if request.query.trim() != request.query || request.query.chars().count() > 512 {
        return Err(DirectoryError::InvalidRequest(
            "query must contain at most 512 characters without surrounding whitespace".to_owned(),
        ));
    }
    if let Some(filters) = &request.filters {
        if filters.keywords.len() > 32
            || filters
                .keywords
                .iter()
                .any(|value| value.is_empty() || value.chars().count() > 64)
        {
            return Err(DirectoryError::InvalidRequest(
                "keywords must contain at most 32 values of at most 64 characters".to_owned(),
            ));
        }
    }
    Ok(())
}

fn consume_response(response: HttpResponse) -> Result<HttpResponse, DirectoryError> {
    if response.body.len() > MAXIMUM_RESPONSE_BYTES {
        return Err(DirectoryError::InvalidResponse(
            "Directory response exceeds 524288 bytes".to_owned(),
        ));
    }
    if !(200..300).contains(&response.status) {
        let message = String::from_utf8_lossy(&response.body).into_owned();
        return Err(DirectoryError::Request {
            headers: response.headers,
            message,
            status: response.status,
        });
    }
    let content_type = response
        .headers
        .get("content-type")
        .map(|value| value.split(';').next().unwrap_or_default().trim())
        .unwrap_or_default();
    if !content_type.eq_ignore_ascii_case("application/json") {
        return Err(DirectoryError::InvalidResponse(
            "Directory response must use application/json".to_owned(),
        ));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;

    struct MockTransport {
        requests: Mutex<Vec<HttpRequest>>,
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
            self.requests.lock().unwrap().push(request);
            Ok(HttpResponse {
                body: br#"{"items":[{"description":"Plants","indexed_at":"2026-08-25T00:00:00Z","language":"en","localizations":["en"],"name":"Indica Flowers","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}],"protocols":{"trust":[{"name":"tap"}]},"service_origin":"https://demo.inflowpay.ai"}]}"#.to_vec(),
                headers: BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]),
                status: 200,
            })
        }
    }

    #[tokio::test]
    async fn searches_only_the_selected_canonical_directory() {
        let transport = Arc::new(MockTransport {
            requests: Mutex::new(Vec::new()),
        });
        let client = DirectoryClient::with_transport(Environment::Sandbox, transport.clone());
        let page = client
            .search(&SearchRequest {
                query: "plants".to_owned(),
                ..SearchRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(page.items[0].name, "Indica Flowers");
        assert_eq!(
            page.items[0].protocols.as_ref().unwrap().trust,
            [odp_core::TrustProtocol {
                name: odp_core::Protocol::Tap
            }]
        );
        let requests = transport.requests.lock().unwrap();
        assert_eq!(
            requests[0].url,
            "https://sandbox.inflowpay.ai/v1/services/search"
        );
        assert_eq!(requests[0].method, "POST");
    }

    #[tokio::test]
    async fn rejects_unbounded_directory_traversal() {
        let client = DirectoryClient::with_transport(
            Environment::Production,
            Arc::new(MockTransport {
                requests: Mutex::new(Vec::new()),
            }),
        );
        let error = client
            .search_services(
                &SearchRequest::default(),
                IterationOptions {
                    max_items: 10_001,
                    max_pages: 0,
                },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("max_items"));
    }
}
