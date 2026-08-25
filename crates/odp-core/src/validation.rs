use std::{collections::BTreeMap, sync::OnceLock};

use include_dir::{Dir, include_dir};
use jsonschema::{Registry, Validator};
use language_tags::LanguageTag;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;

use crate::{
    Collection, CollectionSearchRequest, FilterDefinition, FilterOperator, FilterType, Offering,
    OfferingPage, OfferingSearchRequest, Operation, Page, ProblemDetails, ResourceIdentity,
    ServiceDocument, SortDefinition,
};

static SCHEMA_FILES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/schemas");
static SCHEMAS: OnceLock<Result<SchemaSet, String>> = OnceLock::new();

struct SchemaSet {
    validators: BTreeMap<String, Validator>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidationIssue {
    pub keyword: String,
    pub message: String,
    pub params: BTreeMap<String, Value>,
    pub path: String,
}

#[derive(Clone, Debug, Error, PartialEq)]
#[error("invalid ODP {document_type}")]
pub struct ValidationError {
    pub document_type: String,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("failed to initialize ODP schemas: {0}")]
    SchemaInitialization(String),
}

pub fn parse_service_document(data: &[u8]) -> Result<ServiceDocument, ParseError> {
    parse(
        data,
        "service-document.schema.json",
        "Service Document",
        service_document_issues,
    )
}

pub fn parse_collection(data: &[u8]) -> Result<Collection, ParseError> {
    parse(
        data,
        "collection.schema.json",
        "Collection",
        |value: &Collection| {
            representation_issues(&value.language, &value.localizations, &value.images)
        },
    )
}

pub fn parse_offering(data: &[u8]) -> Result<Offering, ParseError> {
    parse(
        data,
        "offering.schema.json",
        "Offering",
        |value: &Offering| {
            representation_issues(&value.language, &value.localizations, &value.images)
        },
    )
}

pub fn parse_problem_details(data: &[u8]) -> Result<ProblemDetails, ParseError> {
    parse_without_refinement(data, "problem-details.schema.json", "Problem Details")
}

pub fn parse_problem_response(data: &[u8], http_status: u16) -> Result<ProblemDetails, ParseError> {
    let value = parse_problem_details(data)?;
    if value.status != http_status {
        return Err(ValidationError {
            document_type: "Problem Details".to_owned(),
            issues: vec![issue(
                "/status",
                "http-status",
                "must match the HTTP response status",
            )],
        }
        .into());
    }
    Ok(value)
}

pub fn parse_resource_identity(data: &[u8]) -> Result<ResourceIdentity, ParseError> {
    parse_without_refinement(data, "resource-identity.schema.json", "resource identity")
}

pub fn parse_page<T: DeserializeOwned>(data: &[u8]) -> Result<Page<T>, ParseError> {
    parse_without_refinement(data, "page-envelope.schema.json", "page envelope")
}

pub fn parse_collection_search_request(data: &[u8]) -> Result<CollectionSearchRequest, ParseError> {
    parse_without_refinement(
        data,
        "collection-search-request.schema.json",
        "Collection search request",
    )
}

pub fn parse_offering_search_request(data: &[u8]) -> Result<OfferingSearchRequest, ParseError> {
    parse_without_refinement(
        data,
        "offering-search-request.schema.json",
        "Offering search request",
    )
}

pub fn parse_offering_search_response(data: &[u8]) -> Result<OfferingPage<Offering>, ParseError> {
    parse_without_refinement(
        data,
        "offering-search-response.schema.json",
        "Offering search response",
    )
}

pub fn parse_filter_definition(data: &[u8]) -> Result<FilterDefinition, ParseError> {
    parse(
        data,
        "filter-definition.schema.json",
        "Filter Definition",
        filter_definition_issues,
    )
}

pub fn parse_sort_definition(data: &[u8]) -> Result<SortDefinition, ParseError> {
    parse_without_refinement(data, "sort-definition.schema.json", "Sort Definition")
}

pub fn parse_filter_definition_page(data: &[u8]) -> Result<Page<FilterDefinition>, ParseError> {
    parse_without_refinement(
        data,
        "filter-definition-page.schema.json",
        "Filter Definition page",
    )
}

pub fn parse_sort_definition_page(data: &[u8]) -> Result<Page<SortDefinition>, ParseError> {
    parse_without_refinement(
        data,
        "sort-definition-page.schema.json",
        "Sort Definition page",
    )
}

pub fn validate_value(
    value: &Value,
    schema_name: &str,
    document_type: &str,
) -> Result<(), ParseError> {
    let schemas = schemas()?;
    let validator = schemas.validators.get(schema_name).ok_or_else(|| {
        ParseError::SchemaInitialization(format!("missing bundled schema {schema_name}"))
    })?;
    let issues = validator
        .iter_errors(value)
        .map(|error| {
            let schema_path = error.schema_path().to_string();
            ValidationIssue {
                keyword: schema_path
                    .rsplit('/')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("schema")
                    .to_owned(),
                message: error.to_string(),
                params: BTreeMap::new(),
                path: error.instance_path().to_string(),
            }
        })
        .collect::<Vec<_>>();
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ValidationError {
            document_type: document_type.to_owned(),
            issues,
        }
        .into())
    }
}

fn parse_without_refinement<T: DeserializeOwned>(
    data: &[u8],
    schema_name: &str,
    document_type: &str,
) -> Result<T, ParseError> {
    parse(data, schema_name, document_type, |_| Vec::new())
}

fn parse<T: DeserializeOwned>(
    data: &[u8],
    schema_name: &str,
    document_type: &str,
    refine: impl FnOnce(&T) -> Vec<ValidationIssue>,
) -> Result<T, ParseError> {
    let raw = serde_json::from_slice(data).map_err(|error| ValidationError {
        document_type: document_type.to_owned(),
        issues: vec![issue("", "json", &error.to_string())],
    })?;
    validate_value(&raw, schema_name, document_type)?;
    let value = serde_json::from_value(raw).map_err(|error| ValidationError {
        document_type: document_type.to_owned(),
        issues: vec![issue("", "decode", &error.to_string())],
    })?;
    let issues = refine(&value);
    if issues.is_empty() {
        Ok(value)
    } else {
        Err(ValidationError {
            document_type: document_type.to_owned(),
            issues,
        }
        .into())
    }
}

fn schemas() -> Result<&'static SchemaSet, ParseError> {
    SCHEMAS
        .get_or_init(initialize_schemas)
        .as_ref()
        .map_err(|error| ParseError::SchemaInitialization(error.clone()))
}

fn initialize_schemas() -> Result<SchemaSet, String> {
    let mut documents = BTreeMap::new();
    for file in SCHEMA_FILES.files() {
        let name = file
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "bundled schema has an invalid name".to_owned())?;
        let value = serde_json::from_slice(file.contents())
            .map_err(|error| format!("decode {name}: {error}"))?;
        documents.insert(name.to_owned(), value);
    }
    let mut registry = Registry::new();
    for (name, value) in &documents {
        registry = registry
            .add(
                format!("https://offeringprotocol.org/schemas/{name}"),
                value,
            )
            .map_err(|error| format!("register {name}: {error}"))?;
    }
    let registry = registry
        .prepare()
        .map_err(|error| format!("prepare schema registry: {error}"))?;
    let mut validators = BTreeMap::new();
    for (name, value) in &documents {
        let validator = jsonschema::options()
            .with_registry(&registry)
            .should_validate_formats(true)
            .build(value)
            .map_err(|error| format!("compile {name}: {error}"))?;
        validators.insert(name.clone(), validator);
    }
    Ok(SchemaSet { validators })
}

fn service_document_issues(value: &ServiceDocument) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if value.additional.contains_key("id") {
        issues.push(issue(
            "/id",
            "prohibited",
            "must not appear in a Service Document",
        ));
    }
    if value.additional.contains_key("web_url") {
        issues.push(issue(
            "/web_url",
            "prohibited",
            "must not appear in a Service Document",
        ));
    }
    if !valid_language_tag(&value.language) {
        issues.push(issue("/language", "language-tag", "must be a language tag"));
    }
    validate_localizations(&value.language, &value.localizations, true, &mut issues);
    if value
        .keywords
        .iter()
        .map(|keyword| keyword.chars().count())
        .sum::<usize>()
        > 1024
    {
        issues.push(issue(
            "/keywords",
            "max-code-points",
            "must contain no more than 1024 code points in total",
        ));
    }
    if value.search_capabilities.is_some()
        && !value
            .operations
            .iter()
            .any(|operation| operation.name == Operation::SearchOfferings)
    {
        issues.push(issue(
            "/search_capabilities",
            "operation-support",
            "requires the search-offerings operation",
        ));
    }
    issues
}

fn representation_issues(
    language: &str,
    localizations: &[String],
    images: &[crate::ResourceImage],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if !language.is_empty() && !valid_language_tag(language) {
        issues.push(issue("/language", "language-tag", "must be a language tag"));
    }
    validate_localizations(language, localizations, false, &mut issues);
    let mut sources = std::collections::BTreeSet::new();
    if images.iter().any(|image| !sources.insert(&image.src)) {
        issues.push(issue(
            "/images",
            "unique-image-source",
            "must contain unique image sources",
        ));
    }
    issues
}

fn validate_localizations(
    language: &str,
    localizations: &[String],
    require_default: bool,
    issues: &mut Vec<ValidationIssue>,
) {
    if localizations.iter().any(|tag| !valid_language_tag(tag)) {
        issues.push(issue(
            "/localizations",
            "language-tag",
            "must contain only language tags",
        ));
        return;
    }
    let folded = localizations
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    if folded.len() != localizations.len() {
        issues.push(issue(
            "/localizations",
            "unique-language-tag",
            "must be unique without regard to case",
        ));
    }
    if (require_default || (!language.is_empty() && !localizations.is_empty()))
        && !folded.contains(&language.to_ascii_lowercase())
    {
        issues.push(issue(
            "/localizations",
            if require_default {
                "contains-default-language"
            } else {
                "contains-language"
            },
            if require_default {
                "must contain the default language"
            } else {
                "must contain the representation language"
            },
        ));
    }
}

fn valid_language_tag(value: &str) -> bool {
    let Ok(tag) = value.parse::<LanguageTag>() else {
        return false;
    };
    let mut variants = std::collections::BTreeSet::new();
    if tag
        .variant_subtags()
        .any(|variant| !variants.insert(variant.to_ascii_lowercase()))
    {
        return false;
    }
    let mut extensions = std::collections::BTreeSet::new();
    !tag.extension_subtags()
        .any(|(singleton, _)| !extensions.insert(singleton.to_ascii_lowercase()))
}

fn filter_definition_issues(value: &FilterDefinition) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if matches!(value.filter_type, FilterType::String | FilterType::Boolean)
        && value.operators.iter().any(|operator| {
            matches!(
                operator,
                FilterOperator::GreaterThan
                    | FilterOperator::GreaterThanOrEqual
                    | FilterOperator::LessThan
                    | FilterOperator::LessThanOrEqual
            )
        })
    {
        issues.push(issue(
            "/operators",
            "operator-type",
            "contains an operator incompatible with the Filter type",
        ));
    }
    if value.filter_type == FilterType::Boolean && value.unit.is_some() {
        issues.push(issue(
            "/unit",
            "unit-type",
            "must not appear on a boolean Filter",
        ));
    }
    issues
}

fn issue(path: &str, keyword: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        keyword: keyword.to_owned(),
        message: message.to_owned(),
        params: BTreeMap::new(),
        path: path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normative_service_document() {
        let document = br#"{
            "description":"Plant store",
            "http":{"endpoint_base":"/odp"},
            "language":"en",
            "localizations":["en"],
            "name":"Plants",
            "odp_version":"1.0",
            "operations":[
                {"authentication":"not-required","name":"get-offering"},
                {"authentication":"not-required","name":"list-offerings"}
            ]
        }"#;
        assert_eq!(parse_service_document(document).unwrap().name, "Plants");
    }

    #[test]
    fn returns_structured_validation_issues() {
        let error = parse_service_document(br#"{}"#).unwrap_err();
        let ParseError::Validation(error) = error else {
            panic!("expected validation error");
        };
        assert!(!error.issues.is_empty());
    }

    #[test]
    fn rejects_duplicate_language_variants() {
        let document = br#"{
            "description":"An example Service.",
            "http":{"endpoint_base":"/odp"},
            "language":"sl-rozaj-rozaj",
            "localizations":["sl-rozaj-rozaj"],
            "name":"Example",
            "odp_version":"1.0",
            "operations":[
                {"authentication":"not-required","name":"get-offering"},
                {"authentication":"not-required","name":"list-offerings"}
            ]
        }"#;
        assert!(parse_service_document(document).is_err());
    }
}
