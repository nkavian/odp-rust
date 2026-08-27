use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime},
};

use odp_core::{
    Collection, CollectionSearchRequest, Offering, OfferingPage, OfferingSearchRequest, Operation,
    Page, ParseError, Representation, ServiceDocument, build_operation_url, derive_service_origin,
    parse_collection, parse_offering, parse_offering_search_response, parse_page,
    parse_problem_response, parse_service_document, resolve_continuation,
};
use odp_directory::{HttpRequest, ReqwestTransport, Transport, TransportError};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;

use crate::{Cache, CacheFallbacks, CacheRecord, default_cache};

const MEDIA_TYPE: &str = "application/odp+json";
const MAX_DOCUMENT_BYTES: usize = 65_536;
const MAX_RESOURCE_BYTES: usize = 524_288;
const MAX_REDIRECTS: usize = 5;
const MAX_TRAVERSAL_ITEMS: usize = 10_000;
const MAX_TRAVERSAL_PAGES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraversalOptions {
    pub max_items: usize,
    pub max_pages: usize,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self {
            max_items: MAX_TRAVERSAL_ITEMS,
            max_pages: MAX_TRAVERSAL_PAGES,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inspection {
    pub document: ServiceDocument,
    pub final_url: String,
    pub freshness: Freshness,
    pub requested_url: String,
    pub service_origin: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Freshness {
    Fetched,
    Fresh,
    Revalidated,
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("invalid Agent request: {0}")]
    InvalidRequest(String),
    #[error("invalid ODP response: {0}")]
    InvalidResponse(String),
    #[error("ODP cache failed: {0}")]
    Cache(String),
    #[error("ODP Directory failed: {0}")]
    Directory(String),
    #[error("ODP Service does not advertise {0:?}")]
    UnsupportedOperation(Operation),
    #[error("ODP request failed with HTTP {status}: {message}")]
    Request { message: String, status: u16 },
}

#[derive(Clone)]
pub struct ServiceClient {
    accept_language: Option<String>,
    cache: Arc<dyn Cache>,
    cache_fallbacks: CacheFallbacks,
    cache_partition: String,
    service_origin: String,
    supporting_transport: Arc<dyn Transport>,
    transport: Arc<dyn Transport>,
}

impl ServiceClient {
    pub fn new(service_url: &str) -> Result<Self, AgentError> {
        Self::with_transport(service_url, Arc::new(ReqwestTransport::new()?))
    }

    pub fn with_transport(
        service_url: &str,
        transport: Arc<dyn Transport>,
    ) -> Result<Self, AgentError> {
        let supporting_transport = Arc::new(ReqwestTransport::new()?);
        Ok(Self {
            accept_language: None,
            cache: default_cache(),
            cache_fallbacks: CacheFallbacks::default(),
            cache_partition: "anonymous".to_owned(),
            service_origin: derive_service_origin(service_url)
                .map_err(|error| AgentError::InvalidRequest(error.to_string()))?,
            supporting_transport,
            transport,
        })
    }

    pub fn with_accept_language(mut self, language: impl Into<String>) -> Self {
        self.accept_language = Some(language.into());
        self
    }

    pub fn with_cache(mut self, cache: Arc<dyn Cache>) -> Self {
        self.cache = cache;
        self
    }

    pub fn with_cache_fallbacks(mut self, fallbacks: CacheFallbacks) -> Self {
        self.cache_fallbacks = fallbacks;
        self
    }

    pub fn with_cache_partition(mut self, partition: impl Into<String>) -> Self {
        self.cache_partition = partition.into();
        self
    }

    pub fn with_supporting_transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.supporting_transport = transport;
        self
    }

    pub async fn inspect(&self) -> Result<Inspection, AgentError> {
        let requested_url = format!("{}/.well-known/odp", self.service_origin);
        let target = Url::parse(&requested_url)
            .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        let response = self
            .request_cached(
                "GET",
                target,
                Vec::new(),
                MAX_DOCUMENT_BYTES,
                self.cache_fallbacks.service_document,
                validate_service_document_bytes,
            )
            .await?;
        let document = parse_service_document(&response.body)?;
        Ok(Inspection {
            document,
            final_url: response.final_url,
            freshness: response.freshness,
            requested_url,
            service_origin: self.service_origin.clone(),
        })
    }

    pub async fn list_collections(
        &self,
        representation: Representation,
        limit: usize,
    ) -> Result<Page<Collection>, AgentError> {
        let page = self
            .get_page(Operation::ListCollections, None, representation, limit)
            .await?;
        validate_collections(page)
    }

    pub async fn get_collection(&self, id: &str) -> Result<Collection, AgentError> {
        let data = self
            .get_resource(Operation::GetCollection, id, Representation::Full)
            .await?;
        Ok(parse_collection(&data)?)
    }

    pub async fn search_collections(
        &self,
        request: &CollectionSearchRequest,
        representation: Representation,
    ) -> Result<Page<Collection>, AgentError> {
        let data = self
            .post_search(
                Operation::SearchCollections,
                serde_json::to_vec(request)
                    .map_err(|error| AgentError::InvalidRequest(error.to_string()))?,
                representation,
            )
            .await?;
        validate_collections(parse_page(&data)?)
    }

    pub async fn list_offerings(
        &self,
        representation: Representation,
        limit: usize,
    ) -> Result<OfferingPage<Offering>, AgentError> {
        self.get_offering_page(Operation::ListOfferings, None, representation, limit)
            .await
    }

    pub async fn list_collection_offerings(
        &self,
        collection_id: &str,
        representation: Representation,
        limit: usize,
    ) -> Result<OfferingPage<Offering>, AgentError> {
        self.get_offering_page(
            Operation::ListCollectionOfferings,
            Some(collection_id),
            representation,
            limit,
        )
        .await
    }

    pub async fn get_offering(&self, id: &str) -> Result<Offering, AgentError> {
        let data = self
            .get_resource(Operation::GetOffering, id, Representation::Full)
            .await?;
        Ok(parse_offering(&data)?)
    }

    pub async fn search_offerings(
        &self,
        request: &OfferingSearchRequest,
        representation: Representation,
    ) -> Result<OfferingPage<Offering>, AgentError> {
        let data = self
            .post_search(
                Operation::SearchOfferings,
                serde_json::to_vec(request)
                    .map_err(|error| AgentError::InvalidRequest(error.to_string()))?,
                representation,
            )
            .await?;
        Ok(parse_offering_search_response(&data)?)
    }

    pub async fn continue_collections(&self, next: &str) -> Result<Page<Collection>, AgentError> {
        let target = resolve_continuation(next, &self.service_origin)
            .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        let response = self
            .request_cached(
                "GET",
                target,
                Vec::new(),
                MAX_RESOURCE_BYTES,
                self.cache_fallbacks.collection,
                validate_collection_page_bytes,
            )
            .await?;
        validate_collections(parse_page(&response.body)?)
    }

    pub async fn continue_offerings(
        &self,
        next: &str,
    ) -> Result<OfferingPage<Offering>, AgentError> {
        let target = resolve_continuation(next, &self.service_origin)
            .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        let response = self
            .request_cached(
                "GET",
                target,
                Vec::new(),
                MAX_RESOURCE_BYTES,
                self.cache_fallbacks.offering,
                validate_offering_page_bytes,
            )
            .await?;
        Ok(parse_offering_search_response(&response.body)?)
    }

    pub async fn list_all_collections(
        &self,
        representation: Representation,
        limit: usize,
        options: TraversalOptions,
    ) -> Result<Vec<Collection>, AgentError> {
        let mut page = self.list_collections(representation, limit).await?;
        self.collect_collections(&mut page, options).await
    }

    pub async fn list_all_offerings(
        &self,
        representation: Representation,
        limit: usize,
        options: TraversalOptions,
    ) -> Result<Vec<Offering>, AgentError> {
        let mut page = self.list_offerings(representation, limit).await?;
        self.collect_offerings(&mut page, options).await
    }

    pub async fn search_all_offerings(
        &self,
        request: &OfferingSearchRequest,
        representation: Representation,
        options: TraversalOptions,
    ) -> Result<Vec<Offering>, AgentError> {
        let mut page = self.search_offerings(request, representation).await?;
        self.collect_offerings(&mut page, options).await
    }

    async fn collect_collections(
        &self,
        page: &mut Page<Collection>,
        options: TraversalOptions,
    ) -> Result<Vec<Collection>, AgentError> {
        let (maximum_items, maximum_pages) = traversal_bounds(options)?;
        let mut result = Vec::new();
        for page_number in 0..maximum_pages {
            result.extend(page.items.drain(..).take(maximum_items - result.len()));
            if result.len() == maximum_items || page.next.is_empty() {
                return Ok(result);
            }
            if page_number + 1 < maximum_pages {
                *page = self.continue_collections(&page.next).await?;
            }
        }
        Ok(result)
    }

    async fn collect_offerings(
        &self,
        page: &mut OfferingPage<Offering>,
        options: TraversalOptions,
    ) -> Result<Vec<Offering>, AgentError> {
        let (maximum_items, maximum_pages) = traversal_bounds(options)?;
        let mut result = Vec::new();
        for page_number in 0..maximum_pages {
            result.extend(page.items.drain(..).take(maximum_items - result.len()));
            if result.len() == maximum_items || page.next.is_empty() {
                return Ok(result);
            }
            if page_number + 1 < maximum_pages {
                *page = self.continue_offerings(&page.next).await?;
            }
        }
        Ok(result)
    }

    async fn get_page<T: serde::de::DeserializeOwned>(
        &self,
        operation: Operation,
        id: Option<&str>,
        representation: Representation,
        limit: usize,
    ) -> Result<Page<T>, AgentError> {
        let data = self
            .get_page_bytes(operation, id, representation, limit)
            .await?;
        Ok(parse_page(&data)?)
    }

    async fn get_offering_page(
        &self,
        operation: Operation,
        id: Option<&str>,
        representation: Representation,
        limit: usize,
    ) -> Result<OfferingPage<Offering>, AgentError> {
        let data = self
            .get_page_bytes(operation, id, representation, limit)
            .await?;
        Ok(parse_offering_search_response(&data)?)
    }

    async fn get_page_bytes(
        &self,
        operation: Operation,
        id: Option<&str>,
        representation: Representation,
        limit: usize,
    ) -> Result<Vec<u8>, AgentError> {
        let inspection = self.require_operation(operation).await?;
        let mut target = build_operation_url(
            &inspection.document.http.endpoint_base,
            operation,
            &self.service_origin,
            id,
        )
        .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        target
            .query_pairs_mut()
            .append_pair("representation", representation_name(representation));
        if limit != 0 {
            target
                .query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let fallback = if matches!(
            operation,
            Operation::GetCollection | Operation::ListCollections | Operation::SearchCollections
        ) {
            self.cache_fallbacks.collection
        } else {
            self.cache_fallbacks.offering
        };
        let validator = response_validator(operation);
        Ok(self
            .request_cached(
                "GET",
                target,
                Vec::new(),
                MAX_RESOURCE_BYTES,
                fallback,
                validator,
            )
            .await?
            .body)
    }

    async fn get_resource(
        &self,
        operation: Operation,
        id: &str,
        representation: Representation,
    ) -> Result<Vec<u8>, AgentError> {
        self.get_page_bytes(operation, Some(id), representation, 0)
            .await
    }

    async fn post_search(
        &self,
        operation: Operation,
        body: Vec<u8>,
        representation: Representation,
    ) -> Result<Vec<u8>, AgentError> {
        let inspection = self.require_operation(operation).await?;
        let mut target = build_operation_url(
            &inspection.document.http.endpoint_base,
            operation,
            &self.service_origin,
            None,
        )
        .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        target
            .query_pairs_mut()
            .append_pair("representation", representation_name(representation));
        let fallback = if operation == Operation::SearchCollections {
            self.cache_fallbacks.collection
        } else {
            self.cache_fallbacks.offering
        };
        let validator = response_validator(operation);
        Ok(self
            .request_cached(
                "POST",
                target,
                body,
                MAX_RESOURCE_BYTES,
                fallback,
                validator,
            )
            .await?
            .body)
    }

    async fn require_operation(&self, operation: Operation) -> Result<Inspection, AgentError> {
        let inspection = self.inspect().await?;
        if inspection
            .document
            .operations
            .iter()
            .any(|descriptor| descriptor.name == operation)
        {
            Ok(inspection)
        } else {
            Err(AgentError::UnsupportedOperation(operation))
        }
    }

    async fn request_cached(
        &self,
        method: &str,
        target: Url,
        body: Vec<u8>,
        maximum_bytes: usize,
        fallback: Duration,
        validate: fn(&[u8]) -> Result<(), AgentError>,
    ) -> Result<Response, AgentError> {
        let key = self.cache_key(method, target.as_str(), &body);
        let request_origin = derive_service_origin(target.as_str())
            .map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        let cached = self.cache.get(&key).map_err(AgentError::Cache)?;
        let now = SystemTime::now();
        if let Some(record) = &cached {
            if now < record.expires_at {
                return Ok(Response {
                    body: record.body.clone(),
                    final_url: record.final_url.clone(),
                    freshness: Freshness::Fresh,
                });
            }
        }
        let mut conditional = BTreeMap::new();
        let mut request_target = target;
        if let Some(record) = &cached {
            if let Ok(cached_target) = Url::parse(&record.final_url) {
                if derive_service_origin(cached_target.as_str())
                    .ok()
                    .as_deref()
                    == Some(&request_origin)
                {
                    request_target = cached_target;
                }
            }
            if let Some(etag) = &record.etag {
                conditional.insert("if-none-match".to_owned(), etag.clone());
            }
            if let Some(last_modified) = &record.last_modified {
                conditional.insert("if-modified-since".to_owned(), last_modified.clone());
            }
        }
        let raw = self
            .request_raw(method, request_target, body, conditional, &request_origin)
            .await?;
        if raw.status == 304 {
            let Some(mut record) = cached else {
                return Err(AgentError::InvalidResponse(
                    "ODP response returned 304 without a cached representation".to_owned(),
                ));
            };
            if no_store(&raw.headers) {
                self.cache.delete(&key).map_err(AgentError::Cache)?;
                return Ok(Response {
                    body: record.body,
                    final_url: record.final_url,
                    freshness: Freshness::Revalidated,
                });
            }
            record.expires_at = revalidated_expiration(&raw.headers, &record, fallback, now);
            record.stored_at = now;
            record.final_url = raw.final_url;
            self.cache
                .set(key, record.clone())
                .map_err(AgentError::Cache)?;
            return Ok(Response {
                body: record.body,
                final_url: record.final_url,
                freshness: Freshness::Revalidated,
            });
        }
        let response = consume(raw, maximum_bytes)?;
        validate(&response.body)?;
        if !cacheable(method, &response.headers, fallback) {
            self.cache.delete(&key).map_err(AgentError::Cache)?;
        } else {
            self.cache
                .set(
                    key,
                    CacheRecord {
                        body: response.body.clone(),
                        etag: response.headers.get("etag").cloned(),
                        expires_at: expiration(&response.headers, fallback, now),
                        final_url: response.final_url.clone(),
                        last_modified: response.headers.get("last-modified").cloned(),
                        status: response.status,
                        stored_at: now,
                    },
                )
                .map_err(AgentError::Cache)?;
        }
        Ok(Response {
            body: response.body,
            final_url: response.final_url,
            freshness: Freshness::Fetched,
        })
    }

    async fn request_raw(
        &self,
        mut method: &str,
        mut target: Url,
        mut body: Vec<u8>,
        conditional: BTreeMap<String, String>,
        redirect_origin: &str,
    ) -> Result<RawResponse, AgentError> {
        for redirect in 0..=MAX_REDIRECTS {
            let mut headers = BTreeMap::from([("accept".to_owned(), MEDIA_TYPE.to_owned())]);
            if let Some(language) = &self.accept_language {
                headers.insert("accept-language".to_owned(), language.clone());
            }
            if !body.is_empty() {
                headers.insert("content-type".to_owned(), MEDIA_TYPE.to_owned());
            }
            headers.extend(conditional.clone());
            let response = self
                .transport
                .send(HttpRequest {
                    body: body.clone(),
                    headers,
                    method: method.to_owned(),
                    url: target.to_string(),
                })
                .await?;
            if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                if redirect == MAX_REDIRECTS {
                    return Err(AgentError::InvalidResponse(
                        "ODP response exceeded five redirects".to_owned(),
                    ));
                }
                let location = response.headers.get("location").ok_or_else(|| {
                    AgentError::InvalidResponse("ODP redirect omitted Location".to_owned())
                })?;
                let next = target
                    .join(location)
                    .map_err(|error| AgentError::InvalidResponse(error.to_string()))?;
                if derive_service_origin(next.as_str()).ok().as_deref() != Some(redirect_origin) {
                    return Err(AgentError::InvalidResponse(
                        "ODP redirect changed Service origin".to_owned(),
                    ));
                }
                if response.status == 303
                    || (matches!(response.status, 301 | 302) && method == "POST")
                {
                    method = "GET";
                    body.clear();
                }
                target = next;
                continue;
            }
            return Ok(RawResponse {
                body: response.body,
                final_url: target.to_string(),
                headers: response.headers,
                status: response.status,
            });
        }
        Err(AgentError::InvalidResponse(
            "ODP response exceeded its redirect limit".to_owned(),
        ))
    }

    fn cache_key(&self, method: &str, target: &str, body: &[u8]) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            self.cache_partition,
            method,
            target,
            self.accept_language.as_deref().unwrap_or_default(),
            sha256_hex(body)
        )
    }

    pub(crate) fn service_origin(&self) -> &str {
        &self.service_origin
    }

    pub(crate) async fn linked_odp(
        &self,
        target: Url,
        fallback: Duration,
        validate: fn(&[u8]) -> Result<(), AgentError>,
    ) -> Result<Vec<u8>, AgentError> {
        Ok(self
            .request_cached(
                "GET",
                target,
                Vec::new(),
                MAX_RESOURCE_BYTES,
                fallback,
                validate,
            )
            .await?
            .body)
    }

    pub(crate) async fn supporting_json(
        &self,
        target: &str,
        resource_class: &str,
        accept: &str,
        media_types: &[&str],
        maximum_bytes: usize,
    ) -> Result<serde_json::Value, AgentError> {
        let mut current =
            Url::parse(target).map_err(|error| AgentError::InvalidRequest(error.to_string()))?;
        if current.scheme() != "https" || current.host_str().is_none() {
            return Err(AgentError::InvalidRequest(
                "ODP supporting document URL must use HTTPS".to_owned(),
            ));
        }
        let key = format!("anonymous:{resource_class}\nGET\n{target}\n{accept}");
        let cached = self.cache.get(&key).map_err(AgentError::Cache)?;
        let now = SystemTime::now();
        if let Some(record) = &cached {
            if now < record.expires_at {
                return decode_json_object(&record.body);
            }
        }
        for redirects in 0..=MAX_REDIRECTS {
            let mut headers = BTreeMap::from([("accept".to_owned(), accept.to_owned())]);
            if let Some(record) = &cached {
                if let Some(etag) = &record.etag {
                    headers.insert("if-none-match".to_owned(), etag.clone());
                }
                if let Some(last_modified) = &record.last_modified {
                    headers.insert("if-modified-since".to_owned(), last_modified.clone());
                }
            }
            let response = self
                .supporting_transport
                .send(HttpRequest {
                    body: Vec::new(),
                    headers,
                    method: "GET".to_owned(),
                    url: current.to_string(),
                })
                .await?;
            if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
                if redirects == MAX_REDIRECTS {
                    return Err(AgentError::InvalidResponse(
                        "ODP supporting document exceeded five redirects".to_owned(),
                    ));
                }
                let location = response.headers.get("location").ok_or_else(|| {
                    AgentError::InvalidResponse(
                        "ODP supporting document redirect omitted Location".to_owned(),
                    )
                })?;
                let next = current
                    .join(location)
                    .map_err(|error| AgentError::InvalidResponse(error.to_string()))?;
                if next.scheme() != "https" || next.host_str().is_none() {
                    return Err(AgentError::InvalidResponse(
                        "ODP supporting document redirect must use HTTPS".to_owned(),
                    ));
                }
                current = next;
                continue;
            }
            if response.status == 304 {
                let Some(mut record) = cached.clone() else {
                    return Err(AgentError::InvalidResponse(
                        "ODP supporting document returned 304 without a cached representation"
                            .to_owned(),
                    ));
                };
                if no_store(&response.headers) {
                    self.cache.delete(&key).map_err(AgentError::Cache)?;
                } else {
                    record.expires_at =
                        revalidated_expiration(&response.headers, &record, Duration::ZERO, now);
                    record.stored_at = now;
                    record.final_url = current.to_string();
                    self.cache
                        .set(key.clone(), record.clone())
                        .map_err(AgentError::Cache)?;
                }
                return decode_json_object(&record.body);
            }
            if !(200..300).contains(&response.status) {
                return Err(AgentError::Request {
                    message: format!("ODP supporting document returned HTTP {}", response.status),
                    status: response.status,
                });
            }
            if response.body.len() > maximum_bytes {
                return Err(AgentError::InvalidResponse(
                    "ODP supporting document exceeds its byte limit".to_owned(),
                ));
            }
            let content_type = response
                .headers
                .get("content-type")
                .map(|value| value.split(';').next().unwrap_or_default().trim())
                .unwrap_or_default();
            if !media_types
                .iter()
                .any(|value| content_type.eq_ignore_ascii_case(value))
            {
                return Err(AgentError::InvalidResponse(
                    "ODP supporting document has an unsupported media type".to_owned(),
                ));
            }
            let document = decode_json_object(&response.body)?;
            if !cacheable("GET", &response.headers, Duration::ZERO) {
                self.cache.delete(&key).map_err(AgentError::Cache)?;
            } else {
                self.cache
                    .set(
                        key.clone(),
                        CacheRecord {
                            body: response.body,
                            etag: response.headers.get("etag").cloned(),
                            expires_at: expiration(&response.headers, Duration::ZERO, now),
                            final_url: current.to_string(),
                            last_modified: response.headers.get("last-modified").cloned(),
                            status: response.status,
                            stored_at: now,
                        },
                    )
                    .map_err(AgentError::Cache)?;
            }
            return Ok(document);
        }
        Err(AgentError::InvalidResponse(
            "ODP supporting document exceeded its redirect limit".to_owned(),
        ))
    }
}

fn sha256_hex(data: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(data);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

fn validate_collections(page: Page<Collection>) -> Result<Page<Collection>, AgentError> {
    for collection in &page.items {
        let mut inherited = collection.clone();
        if inherited.odp_version.is_empty() {
            inherited.odp_version.clone_from(&page.odp_version);
        }
        let data = serde_json::to_vec(&inherited)
            .map_err(|error| AgentError::InvalidResponse(error.to_string()))?;
        parse_collection(&data)?;
    }
    Ok(page)
}

fn validate_offerings(page: OfferingPage<Offering>) -> Result<OfferingPage<Offering>, AgentError> {
    for offering in &page.items {
        let mut inherited = offering.clone();
        if inherited.odp_version.is_empty() {
            inherited.odp_version.clone_from(&page.odp_version);
        }
        let data = serde_json::to_vec(&inherited)
            .map_err(|error| AgentError::InvalidResponse(error.to_string()))?;
        parse_offering(&data)?;
    }
    Ok(page)
}

fn validate_service_document_bytes(data: &[u8]) -> Result<(), AgentError> {
    parse_service_document(data)?;
    Ok(())
}

fn validate_collection_bytes(data: &[u8]) -> Result<(), AgentError> {
    parse_collection(data)?;
    Ok(())
}

fn validate_offering_bytes(data: &[u8]) -> Result<(), AgentError> {
    parse_offering(data)?;
    Ok(())
}

fn validate_collection_page_bytes(data: &[u8]) -> Result<(), AgentError> {
    let page = parse_page(data)?;
    validate_collections(page)?;
    Ok(())
}

fn validate_offering_page_bytes(data: &[u8]) -> Result<(), AgentError> {
    validate_offerings(parse_offering_search_response(data)?)?;
    Ok(())
}

fn response_validator(operation: Operation) -> fn(&[u8]) -> Result<(), AgentError> {
    match operation {
        Operation::GetCollection => validate_collection_bytes,
        Operation::GetOffering => validate_offering_bytes,
        Operation::ListCollections | Operation::SearchCollections => validate_collection_page_bytes,
        Operation::ListCollectionOfferings
        | Operation::ListOfferings
        | Operation::SearchOfferings => validate_offering_page_bytes,
    }
}

fn traversal_bounds(options: TraversalOptions) -> Result<(usize, usize), AgentError> {
    let maximum_items = if options.max_items == 0 {
        MAX_TRAVERSAL_ITEMS
    } else {
        options.max_items
    };
    let maximum_pages = if options.max_pages == 0 {
        MAX_TRAVERSAL_PAGES
    } else {
        options.max_pages
    };
    if maximum_items > MAX_TRAVERSAL_ITEMS || maximum_pages > MAX_TRAVERSAL_PAGES {
        return Err(AgentError::InvalidRequest(
            "traversal exceeds 10000 items or 16 pages".to_owned(),
        ));
    }
    Ok((maximum_items, maximum_pages))
}

struct Response {
    body: Vec<u8>,
    final_url: String,
    freshness: Freshness,
}

struct RawResponse {
    body: Vec<u8>,
    final_url: String,
    headers: BTreeMap<String, String>,
    status: u16,
}

fn consume(response: RawResponse, maximum_bytes: usize) -> Result<RawResponse, AgentError> {
    if response.body.len() > maximum_bytes {
        return Err(AgentError::InvalidResponse(
            "ODP response exceeds its byte limit".to_owned(),
        ));
    }
    if !(200..300).contains(&response.status) {
        let message = parse_problem_response(&response.body, response.status)
            .map(|problem| {
                if problem.detail.is_empty() {
                    problem.title
                } else {
                    problem.detail
                }
            })
            .unwrap_or_else(|_| String::from_utf8_lossy(&response.body).into_owned());
        return Err(AgentError::Request {
            message,
            status: response.status,
        });
    }
    let content_type = response
        .headers
        .get("content-type")
        .map(|value| value.split(';').next().unwrap_or_default().trim())
        .unwrap_or_default();
    if !content_type.eq_ignore_ascii_case(MEDIA_TYPE) {
        return Err(AgentError::InvalidResponse(format!(
            "ODP response must use {MEDIA_TYPE}"
        )));
    }
    Ok(response)
}

fn expiration(
    headers: &BTreeMap<String, String>,
    fallback: Duration,
    now: SystemTime,
) -> SystemTime {
    let directives = cache_directives(headers);
    if directives.contains_key("no-cache") {
        return now;
    }
    let maximum_age = directives
        .get("max-age")
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs);
    if let Some(mut duration) = maximum_age {
        if let Some(age) = headers
            .get("age")
            .and_then(|value| value.trim().parse::<u64>().ok())
        {
            duration = duration.saturating_sub(Duration::from_secs(age));
        }
        return now.checked_add(duration).unwrap_or(now);
    }
    if let Some(expires) = headers
        .get("expires")
        .and_then(|value| httpdate::parse_http_date(value).ok())
    {
        return expires;
    }
    now.checked_add(fallback).unwrap_or(now)
}

fn revalidated_expiration(
    headers: &BTreeMap<String, String>,
    record: &CacheRecord,
    fallback: Duration,
    now: SystemTime,
) -> SystemTime {
    if has_freshness(headers) {
        expiration(headers, fallback, now)
    } else {
        let lifetime = record
            .expires_at
            .duration_since(record.stored_at)
            .unwrap_or(Duration::ZERO);
        now.checked_add(lifetime).unwrap_or(now)
    }
}

fn no_store(headers: &BTreeMap<String, String>) -> bool {
    cache_directives(headers).contains_key("no-store")
}

fn cacheable(method: &str, headers: &BTreeMap<String, String>, fallback: Duration) -> bool {
    if !matches!(method, "GET" | "POST") || !supported_vary(headers) || no_store(headers) {
        return false;
    }
    let directives = cache_directives(headers);
    let no_cache = directives.contains_key("no-cache");
    (method == "GET" && (!fallback.is_zero() || no_cache)) || explicit_freshness(headers)
}

fn supported_vary(headers: &BTreeMap<String, String>) -> bool {
    headers.get("vary").is_none_or(|value| {
        value.split(',').all(|name| {
            matches!(
                name.trim().to_ascii_lowercase().as_str(),
                "" | "accept" | "accept-language" | "content-type"
            )
        })
    })
}

fn explicit_freshness(headers: &BTreeMap<String, String>) -> bool {
    cache_directives(headers).contains_key("max-age") || headers.contains_key("expires")
}

fn has_freshness(headers: &BTreeMap<String, String>) -> bool {
    let directives = cache_directives(headers);
    directives.contains_key("max-age")
        || directives.contains_key("no-cache")
        || directives.contains_key("no-store")
        || headers.contains_key("expires")
}

fn cache_directives(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .get("cache-control")
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let (name, setting) = value.split_once('=').unwrap_or((value, ""));
            (
                name.to_ascii_lowercase(),
                setting.trim_matches('"').to_owned(),
            )
        })
        .collect()
}

fn decode_json_object(data: &[u8]) -> Result<serde_json::Value, AgentError> {
    let value = serde_json::from_slice::<serde_json::Value>(data)
        .map_err(|error| AgentError::InvalidResponse(error.to_string()))?;
    if !value.is_object() {
        return Err(AgentError::InvalidResponse(
            "ODP supporting document must be a JSON object".to_owned(),
        ));
    }
    Ok(value)
}

const fn representation_name(value: Representation) -> &'static str {
    match value {
        Representation::Terse => "terse",
        Representation::Full => "full",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use async_trait::async_trait;
    use odp_directory::HttpResponse;

    use super::*;

    struct MockTransport {
        responses: Mutex<VecDeque<HttpResponse>>,
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, TransportError> {
            Ok(self.responses.lock().unwrap().pop_front().unwrap())
        }
    }

    fn response(body: &'static [u8]) -> HttpResponse {
        HttpResponse {
            body: body.to_vec(),
            headers: BTreeMap::from([("content-type".to_owned(), MEDIA_TYPE.to_owned())]),
            status: 200,
        }
    }

    struct ConditionalTransport {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Transport for ConditionalTransport {
        async fn send(&self, request: HttpRequest) -> Result<HttpResponse, TransportError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            if call == 0 {
                return Ok(HttpResponse {
                    body: br#"{"description":"Plants","http":{"endpoint_base":"/odp"},"language":"en","localizations":["en"],"name":"Indica Flowers","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}]}"#.to_vec(),
                    headers: BTreeMap::from([
                        ("cache-control".to_owned(), "max-age=0".to_owned()),
                        ("content-type".to_owned(), MEDIA_TYPE.to_owned()),
                        ("etag".to_owned(), "document-1".to_owned()),
                    ]),
                    status: 200,
                });
            }
            assert_eq!(
                request.headers.get("if-none-match").map(String::as_str),
                Some("document-1")
            );
            Ok(HttpResponse {
                body: Vec::new(),
                headers: BTreeMap::from([("cache-control".to_owned(), "max-age=60".to_owned())]),
                status: 304,
            })
        }
    }

    struct InvalidTransport {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Transport for InvalidTransport {
        async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, TransportError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(response(br#"{}"#))
        }
    }

    #[tokio::test]
    async fn inspects_support_before_getting_an_offering() {
        let document = br#"{"description":"Plants","http":{"endpoint_base":"/odp"},"language":"en","localizations":["en"],"name":"Indica Flowers","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}]}"#;
        let offering = br#"{"id":"plant-1","name":"Plant","odp_version":"1.0"}"#;
        let client = ServiceClient::with_transport(
            "https://demo.inflowpay.ai",
            Arc::new(MockTransport {
                responses: Mutex::new(VecDeque::from([response(document), response(offering)])),
            }),
        )
        .unwrap();
        assert_eq!(client.get_offering("plant-1").await.unwrap().name, "Plant");
    }

    #[tokio::test]
    async fn caches_the_service_document_with_its_resource_fallback() {
        let document = br#"{"description":"Plants","http":{"endpoint_base":"/odp"},"language":"en","localizations":["en"],"name":"Indica Flowers","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}]}"#;
        let client = ServiceClient::with_transport(
            "https://demo.inflowpay.ai",
            Arc::new(MockTransport {
                responses: Mutex::new(VecDeque::from([response(document)])),
            }),
        )
        .unwrap();
        assert_eq!(
            client.inspect().await.unwrap().freshness,
            Freshness::Fetched
        );
        assert_eq!(client.inspect().await.unwrap().freshness, Freshness::Fresh);
    }

    #[tokio::test]
    async fn revalidates_a_stale_cached_document() {
        let transport = Arc::new(ConditionalTransport {
            calls: AtomicUsize::new(0),
        });
        let client =
            ServiceClient::with_transport("https://demo.inflowpay.ai", transport.clone()).unwrap();
        assert_eq!(
            client.inspect().await.unwrap().freshness,
            Freshness::Fetched
        );
        assert_eq!(
            client.inspect().await.unwrap().freshness,
            Freshness::Revalidated
        );
        assert_eq!(client.inspect().await.unwrap().freshness, Freshness::Fresh);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_cache_an_invalid_document() {
        let transport = Arc::new(InvalidTransport {
            calls: AtomicUsize::new(0),
        });
        let client =
            ServiceClient::with_transport("https://demo.inflowpay.ai", transport.clone()).unwrap();
        assert!(client.inspect().await.is_err());
        assert!(client.inspect().await.is_err());
        assert_eq!(transport.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn post_search_requires_explicit_freshness_before_caching() {
        let headers = BTreeMap::new();
        assert!(!cacheable("POST", &headers, Duration::from_secs(300)));
        let headers = BTreeMap::from([("cache-control".to_owned(), "max-age=30".to_owned())]);
        assert!(cacheable("POST", &headers, Duration::from_secs(300)));
    }

    #[test]
    fn validates_embedded_representations_with_the_page_version() {
        assert!(
            validate_collection_page_bytes(
                br#"{"items":[{"id":"plants","name":"Plants"}],"odp_version":"1.0"}"#
            )
            .is_ok()
        );
        assert!(
            validate_offering_page_bytes(
                br#"{"items":[{"id":"plant","name":"Plant"}],"odp_version":"1.0"}"#
            )
            .is_ok()
        );
        assert!(
            validate_offering_page_bytes(
                br#"{"items":[{"id":"bad/id","name":"Plant"}],"odp_version":"1.0"}"#
            )
            .is_err()
        );
    }
}
