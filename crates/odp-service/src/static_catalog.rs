use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use odp_core::{Collection, Offering, OfferingPage, Operation, Page, VERSION};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;

use crate::{Catalog, CatalogRequest, ServiceError};

const DEFAULT_PAGE_LIMIT: usize = 50;
const CONTINUATION_LIFETIME: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, Default)]
pub struct StaticCatalogOptions {
    pub collections: Vec<Collection>,
    pub offerings: Vec<Offering>,
}

pub struct StaticCatalog {
    collections: Vec<Collection>,
    collection_by_id: BTreeMap<String, Collection>,
    offerings: Vec<Offering>,
    offering_by_id: BTreeMap<String, Offering>,
}

impl StaticCatalog {
    pub fn new(options: StaticCatalogOptions) -> Result<Self, ServiceError> {
        let mut offering_by_id = BTreeMap::new();
        for offering in &options.offerings {
            validate_offering(offering)?;
            if offering_by_id
                .insert(offering.id.clone(), offering.clone())
                .is_some()
            {
                return Err(ServiceError::InvalidConfiguration(
                    "Offering identifiers must be unique".to_owned(),
                ));
            }
        }
        let mut collection_by_id = BTreeMap::new();
        for collection in &options.collections {
            validate_collection(collection)?;
            if collection_by_id
                .insert(collection.id.clone(), collection.clone())
                .is_some()
            {
                return Err(ServiceError::InvalidConfiguration(
                    "Collection identifiers must be unique".to_owned(),
                ));
            }
        }
        for offering in &options.offerings {
            if offering
                .collection_ids
                .iter()
                .any(|id| !collection_by_id.contains_key(id))
            {
                return Err(ServiceError::InvalidConfiguration(format!(
                    "Offering {} refers to an unknown Collection",
                    offering.id
                )));
            }
        }
        Ok(Self {
            collections: options.collections,
            collection_by_id,
            offerings: options.offerings,
            offering_by_id,
        })
    }
}

#[async_trait]
impl Catalog for StaticCatalog {
    fn operations(&self) -> Vec<Operation> {
        let mut operations = vec![Operation::GetOffering, Operation::ListOfferings];
        if !self.collections.is_empty() {
            operations.extend([
                Operation::GetCollection,
                Operation::ListCollectionOfferings,
                Operation::ListCollections,
            ]);
        }
        operations
    }

    async fn list_offerings(
        &self,
        request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        let (items, next) = page(&self.offerings, &request)?;
        Ok(OfferingPage {
            additional: BTreeMap::new(),
            auth_expands: false,
            items,
            next,
            odp_version: VERSION.to_owned(),
            refinements: Vec::new(),
        })
    }

    async fn get_offering(
        &self,
        id: &str,
        _request: CatalogRequest,
    ) -> Result<Option<Offering>, ServiceError> {
        Ok(self.offering_by_id.get(id).cloned())
    }

    async fn list_collections(
        &self,
        request: CatalogRequest,
    ) -> Result<Page<Collection>, ServiceError> {
        let (items, next) = page(&self.collections, &request)?;
        Ok(Page {
            additional: BTreeMap::new(),
            auth_expands: false,
            items,
            next,
            odp_version: VERSION.to_owned(),
        })
    }

    async fn get_collection(
        &self,
        id: &str,
        _request: CatalogRequest,
    ) -> Result<Option<Collection>, ServiceError> {
        Ok(self.collection_by_id.get(id).cloned())
    }

    async fn list_collection_offerings(
        &self,
        collection_id: &str,
        request: CatalogRequest,
    ) -> Result<OfferingPage<Offering>, ServiceError> {
        if !self.collection_by_id.contains_key(collection_id) {
            return Err(ServiceError::Request {
                code: "NOT_FOUND",
                message: "Collection not found".to_owned(),
                status: 404,
            });
        }
        let offerings = self
            .offerings
            .iter()
            .filter(|offering| offering.collection_ids.iter().any(|id| id == collection_id))
            .cloned()
            .collect::<Vec<_>>();
        let (items, next) = page(&offerings, &request)?;
        Ok(OfferingPage {
            additional: BTreeMap::new(),
            auth_expands: false,
            items,
            next,
            odp_version: VERSION.to_owned(),
            refinements: Vec::new(),
        })
    }
}

#[derive(Deserialize, Serialize)]
struct Cursor {
    expires: u64,
    limit: usize,
    offset: usize,
    path: String,
    representation: String,
}

fn page<T: Clone>(
    values: &[T],
    request: &CatalogRequest,
) -> Result<(Vec<T>, String), ServiceError> {
    let limit = if request.limit == 0 {
        DEFAULT_PAGE_LIMIT
    } else {
        request.limit
    };
    let offset = decode_cursor(request, limit)?;
    if offset > values.len() {
        return Err(invalid_cursor());
    }
    let end = (offset + limit).min(values.len());
    let next = if end < values.len() {
        encode_cursor(request, limit, end)?
    } else {
        String::new()
    };
    Ok((values[offset..end].to_vec(), next))
}

fn encode_cursor(
    request: &CatalogRequest,
    limit: usize,
    offset: usize,
) -> Result<String, ServiceError> {
    let expires = SystemTime::now()
        .checked_add(CONTINUATION_LIFETIME)
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .ok_or_else(|| ServiceError::Catalog("system clock is unavailable".to_owned()))?;
    let value = Cursor {
        expires,
        limit,
        offset,
        path: request.path.clone(),
        representation: representation_name(request).to_owned(),
    };
    let data =
        serde_json::to_vec(&value).map_err(|error| ServiceError::Catalog(error.to_string()))?;
    let cursor = URL_SAFE_NO_PAD.encode(data);
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("cursor", &cursor)
        .append_pair("limit", &limit.to_string())
        .append_pair("representation", representation_name(request))
        .finish();
    Ok(format!("{}?{query}", request.path))
}

fn decode_cursor(request: &CatalogRequest, limit: usize) -> Result<usize, ServiceError> {
    let Some(cursor) = &request.cursor else {
        return Ok(0);
    };
    let data = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| invalid_cursor())?;
    let value = serde_json::from_slice::<Cursor>(&data).map_err(|_| invalid_cursor())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid_cursor())?
        .as_secs();
    if value.expires < now
        || value.limit != limit
        || value.path != request.path
        || value.representation != representation_name(request)
    {
        return Err(invalid_cursor());
    }
    Ok(value.offset)
}

fn invalid_cursor() -> ServiceError {
    ServiceError::Request {
        code: "CONTINUATION_UNAVAILABLE",
        message: "Continuation is unavailable".to_owned(),
        status: 410,
    }
}

fn representation_name(request: &CatalogRequest) -> &'static str {
    match request.representation {
        odp_core::Representation::Terse => "terse",
        odp_core::Representation::Full => "full",
    }
}

fn validate_offering(offering: &Offering) -> Result<(), ServiceError> {
    let data = serde_json::to_vec(offering)
        .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
    odp_core::parse_offering(&data)
        .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
    Ok(())
}

fn validate_collection(collection: &Collection) -> Result<(), ServiceError> {
    let data = serde_json::to_vec(collection)
        .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
    odp_core::parse_collection(&data)
        .map_err(|error| ServiceError::InvalidConfiguration(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use odp_core::parse_offering;

    use super::*;

    fn offering(id: &str) -> Offering {
        parse_offering(
            format!(r#"{{"id":"{id}","name":"Plant {id}","odp_version":"1.0"}}"#).as_bytes(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn provides_bounded_stateless_pages() {
        let catalog = StaticCatalog::new(StaticCatalogOptions {
            collections: Vec::new(),
            offerings: vec![offering("one"), offering("two")],
        })
        .unwrap();
        let first = catalog
            .list_offerings(CatalogRequest {
                limit: 1,
                path: "/odp/offerings".to_owned(),
                ..CatalogRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(first.items[0].id, "one");
        let cursor = form_urlencoded::parse(first.next.split_once('?').unwrap().1.as_bytes())
            .find_map(|(name, value)| (name == "cursor").then(|| value.into_owned()))
            .unwrap();
        let second = catalog
            .list_offerings(CatalogRequest {
                cursor: Some(cursor),
                limit: 1,
                path: "/odp/offerings".to_owned(),
                ..CatalogRequest::default()
            })
            .await
            .unwrap();
        assert_eq!(second.items[0].id, "two");
        assert!(second.next.is_empty());
    }
}
