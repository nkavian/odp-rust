use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use odp_agent::{Agent, AgentError, ServiceClient, ServiceClientFactory};
use odp_core::ServiceDocument;
use odp_directory::{
    DirectoryClient, DirectoryService, Environment, HttpRequest, HttpResponse, Transport,
    TransportError,
};
use serde_json::json;

pub struct DirectoryEntry {
    pub client: ServiceClient,
    pub document: ServiceDocument,
    pub origin: String,
}

pub struct MockDirectory {
    pub agent: Agent,
    pub entries: Vec<DirectoryEntry>,
}

struct DirectoryTransport {
    body: Vec<u8>,
}

#[async_trait]
impl Transport for DirectoryTransport {
    async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            body: self.body.clone(),
            headers: BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]),
            status: 200,
        })
    }
}

struct NetworkServiceClientFactory;

impl ServiceClientFactory for NetworkServiceClientFactory {
    fn create(&self, service: &DirectoryService) -> Result<ServiceClient, AgentError> {
        ServiceClient::new(&service.service_origin)
    }
}

pub async fn discover(candidates: &[String]) -> Result<MockDirectory, serde_json::Error> {
    let mut entries = Vec::new();
    for candidate in candidates {
        let Ok(client) = ServiceClient::new(candidate) else {
            continue;
        };
        let Ok(inspection) = client.inspect().await else {
            continue;
        };
        entries.push(DirectoryEntry {
            client,
            document: inspection.document,
            origin: inspection.service_origin,
        });
    }
    let items = entries
        .iter()
        .map(|entry| {
            json!({
                "description": entry.document.description,
                "documentation_url": entry.document.documentation_url,
                "indexed_at": "2026-01-01T00:00:00Z",
                "keywords": entry.document.keywords,
                "language": entry.document.language,
                "localizations": entry.document.localizations,
                "name": entry.document.name,
                "operations": entry.document.operations,
                "protocols": entry.document.protocols,
                "service_origin": entry.origin,
                "status_url": entry.document.status_url,
                "support_url": entry.document.support_url,
                "website_url": entry.document.website_url
            })
        })
        .collect::<Vec<_>>();
    let transport = DirectoryTransport {
        body: serde_json::to_vec(&json!({ "items": items }))?,
    };
    let directory = DirectoryClient::with_transport(Environment::Production, Arc::new(transport));
    Ok(MockDirectory {
        agent: Agent::with_clients(directory, Arc::new(NetworkServiceClientFactory)),
        entries,
    })
}
