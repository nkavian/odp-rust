use odp_agent::ServiceClient;
use odp_core::ServiceDocument;

pub struct DirectoryEntry {
    pub client: ServiceClient,
    pub document: ServiceDocument,
    pub origin: String,
}

pub async fn discover(candidates: &[String]) -> Vec<DirectoryEntry> {
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
    entries
}
