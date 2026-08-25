use std::{collections::BTreeMap, io::BufRead, sync::Arc};

use async_trait::async_trait;
use odp_core::{
    Offering, OfferingPage, OfferingSearchRequest, Operation, ResourceIdentity, VERSION,
    derive_service_origin, is_local_resource_identifier, parse_collection,
    parse_collection_search_request, parse_filter_definition, parse_offering,
    parse_offering_search_request, parse_page, parse_problem_response, parse_service_document,
    parse_sort_definition, resolve_continuation, resolve_resource_reference,
};
use odp_service::{Catalog, CatalogRequest, MEDIA_TYPE, Request, Service, ServiceError};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Deserialize)]
struct AdapterRequest {
    case: BTreeMap<String, Value>,
    role: String,
    sequence: u64,
    vector: Vector,
}

#[derive(Deserialize)]
struct Vector {
    subject: String,
}

#[derive(Serialize)]
struct AdapterResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    protocol_version: &'static str,
    sequence: u64,
    status: &'static str,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::io::stdin();
    for line in input.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: AdapterRequest = serde_json::from_str(&line)?;
        let response = evaluate(&request).await;
        println!("{}", serde_json::to_string(&response)?);
    }
    Ok(())
}

async fn evaluate(request: &AdapterRequest) -> AdapterResponse {
    match evaluate_case(&request.vector.subject, &request.case, &request.role).await {
        Ok(Some(true)) => response(request.sequence, "passed", None),
        Ok(Some(false)) => response(
            request.sequence,
            "failed",
            Some("Public API result did not match the vector".to_owned()),
        ),
        Ok(None) => response(
            request.sequence,
            "skipped",
            Some("No public Rust operation maps this vector case".to_owned()),
        ),
        Err(error) => response(
            request.sequence,
            "failed",
            Some(error.chars().take(1024).collect()),
        ),
    }
}

fn response(sequence: u64, status: &'static str, message: Option<String>) -> AdapterResponse {
    AdapterResponse {
        message,
        protocol_version: "1",
        sequence,
        status,
    }
}

async fn evaluate_case(
    subject: &str,
    case: &BTreeMap<String, Value>,
    role: &str,
) -> Result<Option<bool>, String> {
    let valid = field::<bool>(case, "valid").unwrap_or(false);
    match subject {
        "local-identifier" => {
            let value: String = required(case, "value")?;
            Ok(Some(is_local_resource_identifier(&value) == valid))
        }
        "identity-comparison" => {
            let left: ResourceIdentity = required(case, "left")?;
            let right: ResourceIdentity = required(case, "right")?;
            let expected: bool = required(case, "same_identity")?;
            Ok(Some((left == right) == expected))
        }
        "service-origin" => {
            let value: String = required(case, "value")?;
            let canonical = derive_service_origin(&value).is_ok_and(|origin| origin == value);
            Ok(Some(canonical == valid))
        }
        "resource-reference" => {
            let value: String = required(case, "value")?;
            Ok(Some(
                resolve_resource_reference(&value, "https://service.example").is_ok() == valid,
            ))
        }
        "service-document" => parse_result(case, "document", parse_service_document),
        "collection-envelope" => parse_result(case, "document", parse_collection),
        "offering-contract"
            if field::<String>(case, "representation").as_deref() == Some("full") =>
        {
            parse_result(case, "document", parse_offering)
        }
        "offering-contract" => Ok(None),
        "collection-search-contract" if operation(case) == "validate-request" => {
            parse_result(case, "request", parse_collection_search_request)
        }
        "collection-search-contract" => Ok(None),
        "offering-search-contract" if operation(case) == "validate-request" => {
            parse_result(case, "request", parse_offering_search_request)
        }
        "offering-search-contract" => Ok(None),
        "filter-sort-contract" if operation(case) == "validate-definition" => {
            parse_result(case, "definition", parse_filter_definition)
        }
        "filter-sort-contract"
            if operation(case) == "validate-sort" && !case.contains_key("definitions") =>
        {
            parse_result(case, "sort", parse_sort_definition)
        }
        "filter-sort-contract" => Ok(None),
        "pagination-contract" => evaluate_pagination(case),
        "errors-limits-contract" => evaluate_errors_and_limits(case).await,
        "role-baseline" => evaluate_baseline(case, role),
        _ => Ok(None),
    }
}

fn evaluate_pagination(case: &BTreeMap<String, Value>) -> Result<Option<bool>, String> {
    let valid = field::<bool>(case, "valid").unwrap_or(false);
    match operation(case).as_str() {
        "validate-page" => parse_result(case, "page", parse_page::<Value>),
        "validate-limit" => {
            let limit: usize = required(case, "limit")?;
            Ok(Some((1..=100).contains(&limit) == valid))
        }
        "validate-next" => {
            let next: String = required(case, "next")?;
            let origin: String = required(case, "service_origin")?;
            Ok(Some(resolve_continuation(&next, &origin).is_ok() == valid))
        }
        _ => Ok(None),
    }
}

async fn evaluate_errors_and_limits(
    case: &BTreeMap<String, Value>,
) -> Result<Option<bool>, String> {
    let valid = field::<bool>(case, "valid").unwrap_or(false);
    if operation(case) == "validate-problem" {
        let status: u16 = required(case, "http_status")?;
        let problem = raw(case, "problem")?;
        return Ok(Some(
            parse_problem_response(&problem, status).is_ok() == valid,
        ));
    }
    if operation(case) != "validate-limit"
        || field::<String>(case, "resource").as_deref() != Some("request")
    {
        return Ok(None);
    }
    let bytes: usize = required(case, "bytes")?;
    let status = service_request_status(bytes).await?;
    Ok(Some((status == 200) == valid))
}

fn evaluate_baseline(case: &BTreeMap<String, Value>, role: &str) -> Result<Option<bool>, String> {
    if required::<String>(case, "role")? != role {
        return Ok(None);
    }
    let valid = field::<bool>(case, "valid").unwrap_or(false);
    if role == "agent" {
        let behaviors: Vec<String> = required(case, "behaviors")?;
        let required = [
            "enforce-compatibility",
            "enforce-redirect-and-security",
            "follow-pagination",
            "get-offering",
            "handle-errors-and-limits",
            "honor-caching",
            "inspect-service",
            "list-offerings",
            "process-localization",
            "process-representations",
        ];
        return Ok(Some(
            required
                .iter()
                .all(|behavior| behaviors.iter().any(|value| value == behavior))
                == valid,
        ));
    }
    let operations: Vec<Value> = required(case, "operations")?;
    let descriptors = operations
        .into_iter()
        .map(|name| json!({"authentication": "not-required", "name": name}))
        .collect::<Vec<_>>();
    let document = serde_json::to_vec(&json!({
        "description": "Conformance Service",
        "http": {"endpoint_base": "/odp"},
        "language": "en",
        "localizations": ["en"],
        "name": "Conformance Service",
        "odp_version": VERSION,
        "operations": descriptors
    }))
    .map_err(|error| error.to_string())?;
    let list_response = raw(case, "list_response")?;
    let get_response = raw(case, "get_response")?;
    let actual = parse_service_document(&document).is_ok()
        && parse_page::<Offering>(&list_response).is_ok()
        && parse_offering(&get_response).is_ok();
    Ok(Some(actual == valid))
}

fn parse_result<T>(
    case: &BTreeMap<String, Value>,
    name: &str,
    parser: fn(&[u8]) -> Result<T, odp_core::ParseError>,
) -> Result<Option<bool>, String> {
    let valid = field::<bool>(case, "valid").unwrap_or(false);
    Ok(Some(parser(&raw(case, name)?).is_ok() == valid))
}

fn operation(case: &BTreeMap<String, Value>) -> String {
    field(case, "operation").unwrap_or_default()
}

fn field<T: DeserializeOwned>(case: &BTreeMap<String, Value>, name: &str) -> Option<T> {
    case.get(name)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn required<T: DeserializeOwned>(case: &BTreeMap<String, Value>, name: &str) -> Result<T, String> {
    field(case, name).ok_or_else(|| format!("case omitted {name}"))
}

fn raw(case: &BTreeMap<String, Value>, name: &str) -> Result<Vec<u8>, String> {
    serde_json::to_vec(
        case.get(name)
            .ok_or_else(|| format!("case omitted {name}"))?,
    )
    .map_err(|error| error.to_string())
}

struct ConformanceCatalog;

#[async_trait]
impl Catalog for ConformanceCatalog {
    fn operations(&self) -> Vec<Operation> {
        vec![
            Operation::GetOffering,
            Operation::ListOfferings,
            Operation::SearchOfferings,
        ]
    }

    async fn list_offerings(
        &self,
        _request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        Ok(empty_offerings())
    }

    async fn get_offering(
        &self,
        _id: &str,
        _request: CatalogRequest,
    ) -> Result<Option<Offering>, ServiceError> {
        Ok(None)
    }

    async fn search_offerings(
        &self,
        _query: OfferingSearchRequest,
        _request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        Ok(empty_offerings())
    }
}

fn empty_offerings() -> OfferingPage<Offering> {
    OfferingPage {
        additional: BTreeMap::new(),
        auth_expands: false,
        items: Vec::new(),
        next: String::new(),
        odp_version: VERSION.to_owned(),
        refinements: Vec::new(),
    }
}

async fn service_request_status(size: usize) -> Result<u16, String> {
    let document = parse_service_document(
        br#"{"description":"Conformance Service","http":{"endpoint_base":"/odp"},"language":"en","localizations":["en"],"name":"Conformance Service","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"},{"authentication":"not-required","name":"search-offerings"}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let service =
        Service::new(document, Arc::new(ConformanceCatalog)).map_err(|error| error.to_string())?;
    let payload = br#"{"odp_version":"1.0","query":"gpu"}"#;
    let mut body = payload.to_vec();
    body.resize(size, b' ');
    let response = service
        .handle(Request {
            body,
            headers: BTreeMap::from([
                ("accept".to_owned(), MEDIA_TYPE.to_owned()),
                ("content-type".to_owned(), MEDIA_TYPE.to_owned()),
            ]),
            method: "POST".to_owned(),
            path: "/odp/offerings/search".to_owned(),
            query: String::new(),
        })
        .await;
    Ok(response.status)
}
