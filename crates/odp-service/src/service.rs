use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use odp_core::{
    AdditionalMembers, AuthenticationRequirement, Collection, CollectionSearchRequest,
    EnrollmentProtocol, HttpConfiguration, McpEndpoint, Offering, OfferingPage,
    OfferingSearchRequest, Operation, OperationDescriptor, Page, PaymentProtocol, ProblemDetails,
    Representation, SearchCapabilities, ServiceBranding, ServiceDocument, ServiceOpenApi,
    ServiceProtocols, TrustProtocol, VERSION, is_local_resource_identifier, parse_collection,
    parse_collection_search_request, parse_offering, parse_offering_search_request,
    parse_offering_search_response, parse_page, parse_service_document,
};
use serde_json::json;
use thiserror::Error;
use url::form_urlencoded;

pub const MEDIA_TYPE: &str = "application/odp+json";
pub const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";
const MAXIMUM_REQUEST_BYTES: usize = 65_536;
const MAXIMUM_RESOURCE_BYTES: usize = 524_288;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Request {
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
    pub method: String,
    pub path: String,
    pub query: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Response {
    pub body: Vec<u8>,
    pub headers: BTreeMap<String, String>,
    pub status: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogRequest {
    pub accept_language: Option<String>,
    pub cursor: Option<String>,
    pub limit: usize,
    pub path: String,
    pub representation: Representation,
}

impl Default for CatalogRequest {
    fn default() -> Self {
        Self {
            accept_language: None,
            cursor: None,
            limit: 0,
            path: String::new(),
            representation: Representation::Terse,
        }
    }
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid Service configuration: {0}")]
    InvalidConfiguration(String),
    #[error("invalid Service response: {0}")]
    InvalidResponse(String),
    #[error("catalog operation failed: {0}")]
    Catalog(String),
    #[error("{message}")]
    Request {
        code: &'static str,
        message: String,
        status: u16,
    },
}

#[async_trait]
pub trait Catalog: Send + Sync {
    fn operations(&self) -> Vec<Operation>;

    async fn list_offerings(
        &self,
        request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError>;

    async fn get_offering(
        &self,
        id: &str,
        request: CatalogRequest,
    ) -> Result<Option<Offering>, ServiceError>;

    async fn search_offerings(
        &self,
        _query: OfferingSearchRequest,
        _request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        Err(ServiceError::Catalog(
            "search-offerings is unsupported".to_owned(),
        ))
    }

    async fn list_collections(
        &self,
        _request: CatalogRequest,
    ) -> Result<Page<Collection>, ServiceError> {
        Err(ServiceError::Catalog(
            "list-collections is unsupported".to_owned(),
        ))
    }

    async fn get_collection(
        &self,
        _id: &str,
        _request: CatalogRequest,
    ) -> Result<Option<Collection>, ServiceError> {
        Err(ServiceError::Catalog(
            "get-collection is unsupported".to_owned(),
        ))
    }

    async fn search_collections(
        &self,
        _query: CollectionSearchRequest,
        _request: CatalogRequest,
    ) -> Result<Page<Collection>, ServiceError> {
        Err(ServiceError::Catalog(
            "search-collections is unsupported".to_owned(),
        ))
    }

    async fn list_collection_offerings(
        &self,
        _collection_id: &str,
        _request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        Err(ServiceError::Catalog(
            "list-collection-offerings is unsupported".to_owned(),
        ))
    }
}

pub struct ServiceBuilder {
    document: ServiceDocument,
}

impl ServiceBuilder {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        language: impl Into<String>,
        endpoint_base: impl Into<String>,
    ) -> Self {
        let language = language.into();
        Self {
            document: ServiceDocument {
                additional: AdditionalMembers::new(),
                branding: None,
                description: description.into(),
                documentation_url: String::new(),
                http: HttpConfiguration {
                    additional: AdditionalMembers::new(),
                    endpoint_base: endpoint_base.into(),
                    openapi: None,
                },
                keywords: Vec::new(),
                language: language.clone(),
                localizations: vec![language],
                mcp: Vec::new(),
                name: name.into(),
                odp_version: VERSION.to_owned(),
                operations: Vec::new(),
                payment_origins: Vec::new(),
                protocols: None,
                search_capabilities: None,
                status_url: String::new(),
                support_url: String::new(),
                website_url: String::new(),
            },
        }
    }

    pub fn branding(mut self, branding: ServiceBranding) -> Self {
        self.document.branding = Some(branding);
        self
    }

    pub fn documentation_url(mut self, url: impl Into<String>) -> Self {
        self.document.documentation_url = url.into();
        self
    }

    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.document.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    pub fn localizations<I, S>(mut self, localizations: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.document.localizations = localizations.into_iter().map(Into::into).collect();
        self
    }

    pub fn mcp(mut self, endpoints: Vec<McpEndpoint>) -> Self {
        self.document.mcp = endpoints;
        self
    }

    pub fn openapi(mut self, openapi: ServiceOpenApi) -> Self {
        self.document.http.openapi = Some(openapi);
        self
    }

    pub fn operation_authentication(
        mut self,
        operation: Operation,
        authentication: AuthenticationRequirement,
    ) -> Self {
        if let Some(descriptor) = self
            .document
            .operations
            .iter_mut()
            .find(|descriptor| descriptor.name == operation)
        {
            descriptor.authentication = authentication;
        } else {
            self.document.operations.push(OperationDescriptor {
                authentication,
                name: operation,
            });
        }
        self
    }

    pub fn payment_origins<I, S>(mut self, origins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.document.payment_origins = origins.into_iter().map(Into::into).collect();
        self
    }

    pub fn protocols(
        mut self,
        enrollment: Vec<EnrollmentProtocol>,
        payments: Vec<PaymentProtocol>,
    ) -> Self {
        self.document.protocols = Some(ServiceProtocols {
            enrollment,
            payments,
            trust: Vec::new(),
        });
        self
    }

    pub fn trust(mut self, protocols: Vec<TrustProtocol>) -> Self {
        self.document
            .protocols
            .get_or_insert_with(ServiceProtocols::default)
            .trust = protocols;
        self
    }

    pub fn search_capabilities(mut self, capabilities: SearchCapabilities) -> Self {
        self.document.search_capabilities = Some(capabilities);
        self
    }

    pub fn status_url(mut self, url: impl Into<String>) -> Self {
        self.document.status_url = url.into();
        self
    }

    pub fn support_url(mut self, url: impl Into<String>) -> Self {
        self.document.support_url = url.into();
        self
    }

    pub fn website_url(mut self, url: impl Into<String>) -> Self {
        self.document.website_url = url.into();
        self
    }

    pub fn build(self, catalog: Arc<dyn Catalog>) -> Result<Service, ServiceError> {
        Service::new(self.document, catalog)
    }
}

pub struct Service {
    catalog: Arc<dyn Catalog>,
    document: ServiceDocument,
    endpoint_base: String,
}

impl Service {
    pub fn new(
        mut document: ServiceDocument,
        catalog: Arc<dyn Catalog>,
    ) -> Result<Self, ServiceError> {
        let operations = catalog.operations();
        if !operations.contains(&Operation::ListOfferings)
            || !operations.contains(&Operation::GetOffering)
        {
            return Err(ServiceError::InvalidConfiguration(
                "Catalog must support list-offerings and get-offering".to_owned(),
            ));
        }
        document.odp_version = VERSION.to_owned();
        document.operations = operations
            .into_iter()
            .map(|name| OperationDescriptor {
                authentication: document
                    .operations
                    .iter()
                    .find(|descriptor| descriptor.name == name)
                    .map(|descriptor| descriptor.authentication)
                    .unwrap_or(AuthenticationRequirement::NotRequired),
                name,
            })
            .collect();
        let encoded = serde_json::to_vec(&document)
            .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
        document = parse_service_document(&encoded)
            .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
        let endpoint_base = document.http.endpoint_base.trim_end_matches('/').to_owned();
        Ok(Self {
            catalog,
            document,
            endpoint_base,
        })
    }

    pub fn document(&self) -> ServiceDocument {
        self.document.clone()
    }

    pub async fn handle(&self, request: Request) -> Response {
        match self.handle_result(request).await {
            Ok(response) => response,
            Err(ServiceError::Request {
                code,
                message,
                status,
            }) => problem(status, code, &message),
            Err(error) => problem(500, "INTERNAL_ERROR", &error.to_string()),
        }
    }

    async fn handle_result(&self, request: Request) -> Result<Response, ServiceError> {
        if !accepts_odp(request.headers.get("accept").map(String::as_str)) {
            return Ok(problem(
                406,
                "NOT_ACCEPTABLE",
                "Accept must allow application/odp+json",
            ));
        }
        if request.path == "/.well-known/odp" {
            if request.method != "GET" {
                return Ok(problem(
                    405,
                    "METHOD_NOT_ALLOWED",
                    "The Service Document requires GET",
                ));
            }
            return json_response(200, &self.document, MAXIMUM_REQUEST_BYTES);
        }
        let Some(path) = request.path.strip_prefix(&self.endpoint_base) else {
            return Ok(problem(404, "NOT_FOUND", "ODP resource not found"));
        };
        if let Some(operation) = path_operation(request.method.as_str(), path) {
            if !self
                .document
                .operations
                .iter()
                .any(|descriptor| descriptor.name == operation)
            {
                return Ok(problem(404, "NOT_FOUND", "ODP operation is not supported"));
            }
        }
        let input = catalog_request(&request)?;
        match (request.method.as_str(), path) {
            ("GET", "/offerings") => {
                let representation = input.representation;
                let page = self.catalog.list_offerings(input).await?;
                validate_offering_page(&page, false, representation)?;
                json_response(200, &page, MAXIMUM_RESOURCE_BYTES)
            }
            ("POST", "/offerings/search") => {
                let query = decode_offering_search(&request)?;
                let representation = input.representation;
                let page = self.catalog.search_offerings(query, input).await?;
                validate_offering_page(&page, true, representation)?;
                json_response(200, &page, MAXIMUM_RESOURCE_BYTES)
            }
            ("GET", "/collections") => {
                let representation = input.representation;
                let page = self.catalog.list_collections(input).await?;
                validate_collection_page(&page, representation)?;
                json_response(200, &page, MAXIMUM_RESOURCE_BYTES)
            }
            ("POST", "/collections/search") => {
                let query = decode_collection_search(&request)?;
                let representation = input.representation;
                let page = self.catalog.search_collections(query, input).await?;
                validate_collection_page(&page, representation)?;
                json_response(200, &page, MAXIMUM_RESOURCE_BYTES)
            }
            ("GET", _) => self.get_path(path, input).await,
            _ => Ok(problem(
                405,
                "METHOD_NOT_ALLOWED",
                "ODP operation uses a fixed HTTP method",
            )),
        }
    }

    async fn get_path(&self, path: &str, input: CatalogRequest) -> Result<Response, ServiceError> {
        if let Some(id) = path.strip_prefix("/offerings/") {
            if !is_local_resource_identifier(id) {
                return Ok(problem(
                    400,
                    "INVALID_REQUEST",
                    "Offering identifier is invalid",
                ));
            }
            let representation = input.representation;
            return match self.catalog.get_offering(id, input).await? {
                Some(offering) if offering.id == id => {
                    let encoded = serde_json::to_vec(&offering)
                        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
                    parse_offering(&encoded)
                        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
                    validate_offering_representation(&offering, representation)?;
                    json_response(200, &offering, MAXIMUM_RESOURCE_BYTES)
                }
                Some(_) => Err(ServiceError::InvalidResponse(
                    "Offering identifier does not match request path".to_owned(),
                )),
                None => Ok(problem(404, "NOT_FOUND", "Offering not found")),
            };
        }
        if let Some(value) = path.strip_prefix("/collections/") {
            if let Some(id) = value.strip_suffix("/offerings") {
                let representation = input.representation;
                let page = self.catalog.list_collection_offerings(id, input).await?;
                validate_offering_page(&page, false, representation)?;
                return json_response(200, &page, MAXIMUM_RESOURCE_BYTES);
            }
            let representation = input.representation;
            return match self.catalog.get_collection(value, input).await? {
                Some(collection) if collection.id == value => {
                    let encoded = serde_json::to_vec(&collection)
                        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
                    parse_collection(&encoded)
                        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
                    validate_collection_representation(&collection, representation)?;
                    json_response(200, &collection, MAXIMUM_RESOURCE_BYTES)
                }
                Some(_) => Err(ServiceError::InvalidResponse(
                    "Collection identifier does not match request path".to_owned(),
                )),
                None => Ok(problem(404, "NOT_FOUND", "Collection not found")),
            };
        }
        Ok(problem(404, "NOT_FOUND", "ODP resource not found"))
    }
}

fn catalog_request(request: &Request) -> Result<CatalogRequest, ServiceError> {
    let parameters = form_urlencoded::parse(request.query.as_bytes())
        .into_owned()
        .collect::<BTreeMap<_, _>>();
    let representation = match parameters.get("representation").map(String::as_str) {
        None | Some("terse") => Representation::Terse,
        Some("full") => Representation::Full,
        Some(_) => {
            return Err(request_error(
                400,
                "INVALID_REQUEST",
                "representation is invalid",
            ));
        }
    };
    let limit = parameters
        .get("limit")
        .map(|value| value.parse::<usize>())
        .transpose()
        .map_err(|_| request_error(400, "INVALID_REQUEST", "limit is invalid"))?
        .unwrap_or(0);
    if limit > 100 {
        return Err(request_error(400, "INVALID_REQUEST", "limit exceeds 100"));
    }
    Ok(CatalogRequest {
        accept_language: request.headers.get("accept-language").cloned(),
        cursor: parameters.get("cursor").cloned(),
        limit,
        path: request.path.clone(),
        representation,
    })
}

fn validate_search_request(request: &Request) -> Result<(), ServiceError> {
    if request.body.len() > MAXIMUM_REQUEST_BYTES {
        return Err(request_error(
            413,
            "REQUEST_TOO_LARGE",
            "request body is too large",
        ));
    }
    if request
        .headers
        .get("content-type")
        .map(|value| value.split(';').next().unwrap_or_default())
        != Some(MEDIA_TYPE)
    {
        return Err(request_error(
            415,
            "UNSUPPORTED_MEDIA_TYPE",
            &format!("Content-Type must be {MEDIA_TYPE}"),
        ));
    }
    Ok(())
}

fn decode_offering_search(request: &Request) -> Result<OfferingSearchRequest, ServiceError> {
    validate_search_request(request)?;
    parse_offering_search_request(&request.body)
        .map_err(|error| request_error(400, "INVALID_REQUEST", &error.to_string()))
}

fn decode_collection_search(request: &Request) -> Result<CollectionSearchRequest, ServiceError> {
    validate_search_request(request)?;
    parse_collection_search_request(&request.body)
        .map_err(|error| request_error(400, "INVALID_REQUEST", &error.to_string()))
}

fn request_error(status: u16, code: &'static str, message: &str) -> ServiceError {
    ServiceError::Request {
        code,
        message: message.to_owned(),
        status,
    }
}

fn validate_offering_page(
    page: &OfferingPage<Offering>,
    search_response: bool,
    representation: Representation,
) -> Result<(), ServiceError> {
    let encoded = serde_json::to_vec(page)
        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    if search_response {
        parse_offering_search_response(&encoded)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    } else {
        parse_page::<serde_json::Value>(&encoded)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    }
    for offering in &page.items {
        let mut inherited = offering.clone();
        if inherited.odp_version.is_empty() {
            inherited.odp_version.clone_from(&page.odp_version);
        }
        let encoded = serde_json::to_vec(&inherited)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
        parse_offering(&encoded)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
        validate_offering_representation(offering, representation)?;
    }
    Ok(())
}

fn validate_collection_page(
    page: &Page<Collection>,
    representation: Representation,
) -> Result<(), ServiceError> {
    let encoded = serde_json::to_vec(page)
        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    parse_page::<serde_json::Value>(&encoded)
        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    for collection in &page.items {
        let mut inherited = collection.clone();
        if inherited.odp_version.is_empty() {
            inherited.odp_version.clone_from(&page.odp_version);
        }
        let encoded = serde_json::to_vec(&inherited)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
        parse_collection(&encoded)
            .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
        validate_collection_representation(collection, representation)?;
    }
    Ok(())
}

fn validate_offering_representation(
    offering: &Offering,
    representation: Representation,
) -> Result<(), ServiceError> {
    if representation == Representation::Terse && !offering.actions.is_empty() {
        return Err(ServiceError::InvalidResponse(
            "Catalog returned Actions in a Terse Offering".to_owned(),
        ));
    }
    if representation == Representation::Full && !offering.detail_fields.is_empty() {
        return Err(ServiceError::InvalidResponse(
            "Catalog returned detail_fields in a Full Offering".to_owned(),
        ));
    }
    Ok(())
}

fn validate_collection_representation(
    collection: &Collection,
    representation: Representation,
) -> Result<(), ServiceError> {
    if representation == Representation::Full && !collection.detail_fields.is_empty() {
        return Err(ServiceError::InvalidResponse(
            "Catalog returned detail_fields in a Full Collection".to_owned(),
        ));
    }
    Ok(())
}

fn path_operation(method: &str, path: &str) -> Option<Operation> {
    match (method, path) {
        ("GET", "/offerings") => Some(Operation::ListOfferings),
        ("POST", "/offerings/search") => Some(Operation::SearchOfferings),
        ("GET", "/collections") => Some(Operation::ListCollections),
        ("POST", "/collections/search") => Some(Operation::SearchCollections),
        ("GET", value) if value.starts_with("/offerings/") => Some(Operation::GetOffering),
        ("GET", value) if value.starts_with("/collections/") && value.ends_with("/offerings") => {
            Some(Operation::ListCollectionOfferings)
        }
        ("GET", value) if value.starts_with("/collections/") => Some(Operation::GetCollection),
        _ => None,
    }
}

fn json_response<T: serde::Serialize>(
    status: u16,
    value: &T,
    maximum_bytes: usize,
) -> Result<Response, ServiceError> {
    let body = serde_json::to_vec(value)
        .map_err(|error| ServiceError::InvalidResponse(error.to_string()))?;
    if body.len() > maximum_bytes {
        return Err(ServiceError::InvalidResponse(
            "response body is too large".to_owned(),
        ));
    }
    Ok(Response {
        body,
        headers: BTreeMap::from([("content-type".to_owned(), MEDIA_TYPE.to_owned())]),
        status,
    })
}

fn problem(status: u16, code: &str, detail: &str) -> Response {
    let value = ProblemDetails {
        additional: BTreeMap::new(),
        code: code.to_owned(),
        detail: detail.to_owned(),
        instance: String::new(),
        invalid_params: Vec::new(),
        problem_type: "about:blank".to_owned(),
        status,
        title: detail.to_owned(),
    };
    Response {
        body: serde_json::to_vec(&value)
            .unwrap_or_else(|_| json!({"status":500}).to_string().into_bytes()),
        headers: BTreeMap::from([("content-type".to_owned(), PROBLEM_MEDIA_TYPE.to_owned())]),
        status,
    }
}

fn accepts_odp(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        value.split(',').any(|entry| {
            let media_type = entry.split(';').next().unwrap_or_default().trim();
            media_type == "*/*" || media_type.eq_ignore_ascii_case(MEDIA_TYPE)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use odp_core::{
        Action, ActionRelation, AdditionalMembers, HttpActionTarget, HttpConfiguration,
        parse_collection, parse_service_document,
    };

    struct TestCatalog;

    #[async_trait]
    impl Catalog for TestCatalog {
        fn operations(&self) -> Vec<Operation> {
            vec![Operation::GetOffering, Operation::ListOfferings]
        }

        async fn list_offerings(
            &self,
            _request: CatalogRequest,
        ) -> Result<OfferingPage<Offering>, ServiceError> {
            Ok(OfferingPage {
                additional: AdditionalMembers::new(),
                auth_expands: false,
                items: vec![offering()],
                next: String::new(),
                odp_version: VERSION.to_owned(),
                refinements: Vec::new(),
            })
        }

        async fn get_offering(
            &self,
            id: &str,
            _request: CatalogRequest,
        ) -> Result<Option<Offering>, ServiceError> {
            Ok((id == "plant-1").then(offering))
        }
    }

    fn offering() -> Offering {
        Offering {
            actions: Vec::new(),
            additional: AdditionalMembers::new(),
            attributes: BTreeMap::new(),
            auth_expands: false,
            collection_ids: Vec::new(),
            description: "A healthy plant".to_owned(),
            detail_fields: Vec::new(),
            id: "plant-1".to_owned(),
            images: Vec::new(),
            language: "en".to_owned(),
            localizations: vec!["en".to_owned()],
            name: "Plant".to_owned(),
            odp_version: VERSION.to_owned(),
            price: None,
            schema: None,
            web_url: String::new(),
        }
    }

    fn document() -> ServiceDocument {
        parse_service_document(br#"{"description":"Plants","http":{"endpoint_base":"/odp"},"language":"en","localizations":["en"],"name":"Indica Flowers","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}]}"#).unwrap()
    }

    #[test]
    fn builder_derives_version_and_catalog_operations() {
        let service =
            ServiceBuilder::new("Indica Flowers", "An AI-enabled plant store.", "en", "/odp")
                .keywords(["plants", "flowers"])
                .operation_authentication(
                    Operation::GetOffering,
                    AuthenticationRequirement::Required,
                )
                .protocols(
                    vec![EnrollmentProtocol {
                        name: odp_core::Protocol::Aep,
                    }],
                    Vec::new(),
                )
                .trust(vec![TrustProtocol {
                    name: odp_core::Protocol::Tap,
                }])
                .build(Arc::new(TestCatalog))
                .unwrap();
        let document = service.document();
        assert_eq!(document.odp_version, VERSION);
        assert_eq!(document.localizations, ["en"]);
        assert_eq!(document.keywords, ["plants", "flowers"]);
        assert_eq!(document.operations.len(), 2);
        assert_eq!(
            document.protocols.as_ref().unwrap().trust,
            [TrustProtocol {
                name: odp_core::Protocol::Tap
            }]
        );
        assert_eq!(
            document
                .operations
                .iter()
                .find(|descriptor| descriptor.name == Operation::GetOffering)
                .unwrap()
                .authentication,
            AuthenticationRequirement::Required
        );
        assert_eq!(
            document
                .operations
                .iter()
                .find(|descriptor| descriptor.name == Operation::ListOfferings)
                .unwrap()
                .authentication,
            AuthenticationRequirement::NotRequired
        );
    }

    #[tokio::test]
    async fn serves_a_framework_neutral_offering_request() {
        let service = Service::new(document(), Arc::new(TestCatalog)).unwrap();
        let response = service
            .handle(Request {
                headers: BTreeMap::from([("accept".to_owned(), MEDIA_TYPE.to_owned())]),
                method: "GET".to_owned(),
                path: "/odp/offerings/plant-1".to_owned(),
                query: "representation=full".to_owned(),
                ..Request::default()
            })
            .await;
        assert_eq!(response.status, 200);
        assert_eq!(parse_offering(&response.body).unwrap().id, "plant-1");
    }

    #[test]
    fn model_http_configuration_remains_framework_neutral() {
        let configuration = HttpConfiguration {
            additional: AdditionalMembers::new(),
            endpoint_base: "/odp".to_owned(),
            openapi: None,
        };
        assert_eq!(configuration.endpoint_base, "/odp");
    }

    #[test]
    fn rejects_search_bodies_above_the_normative_limit() {
        let error = validate_search_request(&Request {
            body: vec![b' '; MAXIMUM_REQUEST_BYTES + 1],
            headers: BTreeMap::from([("content-type".to_owned(), MEDIA_TYPE.to_owned())]),
            method: "POST".to_owned(),
            path: "/odp/offerings/search".to_owned(),
            query: String::new(),
        })
        .unwrap_err();
        assert!(matches!(error, ServiceError::Request { status: 413, .. }));
    }

    #[test]
    fn rejects_invalid_page_envelopes() {
        let page = OfferingPage {
            additional: AdditionalMembers::new(),
            auth_expands: false,
            items: vec![offering()],
            next: String::new(),
            odp_version: String::new(),
            refinements: Vec::new(),
        };

        assert!(validate_offering_page(&page, false, Representation::Terse).is_err());
    }

    #[test]
    fn rejects_representation_contract_violations() {
        let mut terse = offering();
        terse.actions.push(Action {
            authentication: AuthenticationRequirement::NotRequired,
            description: String::new(),
            http: Some(HttpActionTarget {
                href: "/purchase".to_owned(),
                method: "POST".to_owned(),
                request: None,
                response_content_types: Vec::new(),
            }),
            id: "purchase".to_owned(),
            openapi: None,
            rel: ActionRelation::Purchase,
        });
        assert!(validate_offering_representation(&terse, Representation::Terse).is_err());

        let mut full = offering();
        full.detail_fields.push("/description".to_owned());
        assert!(validate_offering_representation(&full, Representation::Full).is_err());

        let mut collection =
            parse_collection(br#"{"id":"plants","name":"Plants","odp_version":"1.0"}"#).unwrap();
        collection.detail_fields.push("/description".to_owned());
        assert!(validate_collection_representation(&collection, Representation::Full).is_err());
    }
}
