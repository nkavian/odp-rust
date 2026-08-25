use std::{collections::BTreeMap, io, sync::Arc};

use odp_core::{parse_collection, parse_offering, parse_service_document};
use odp_service::{Request, Service, StaticCatalog, StaticCatalogOptions};
use tiny_http::{Header, Response as HttpResponse, Server};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "4104".to_owned());
    let address = format!("127.0.0.1:{port}");
    let service = service()?;
    let server = Server::http(&address)?;
    println!("Small ODP Service listening at http://{address}");
    println!("Service Document: http://{address}/.well-known/odp");
    println!("Offerings: http://{address}/odp/offerings");
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;

    for mut incoming in server.incoming_requests() {
        let method = incoming.method().as_str().to_owned();
        let url = incoming.url().to_owned();
        if method == "GET" && url == "/downloads/incident-plan.txt" {
            incoming.respond(
                HttpResponse::from_string("Incident Response Plan\n")
                    .with_header(header("content-type", "text/plain; charset=utf-8")?),
            )?;
            println!("GET {url} -> 200");
            continue;
        }
        let (path, query) = url.split_once('?').unwrap_or((&url, ""));
        let headers = incoming
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().as_str().to_ascii_lowercase(),
                    header.value.as_str().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut body = Vec::new();
        incoming.as_reader().read_to_end(&mut body)?;
        let response = runtime.block_on(service.handle(Request {
            body,
            headers,
            method: method.clone(),
            path: path.to_owned(),
            query: query.to_owned(),
        }));
        let mut output = HttpResponse::from_data(response.body).with_status_code(response.status);
        for (name, value) in response.headers {
            output.add_header(header(name, value)?);
        }
        incoming.respond(output)?;
        println!("{method} {url} -> {}", response.status);
    }
    Ok(())
}

fn header(
    name: impl Into<Vec<u8>> + AsRef<[u8]>,
    value: impl Into<Vec<u8>> + AsRef<[u8]>,
) -> Result<Header, io::Error> {
    Header::from_bytes(name, value)
        .map_err(|()| io::Error::new(io::ErrorKind::InvalidInput, "invalid HTTP header"))
}

fn service() -> Result<Service, Box<dyn std::error::Error + Send + Sync>> {
    let document = parse_service_document(
        br#"{"description":"Resources and services for ODP integrators.","http":{"endpoint_base":"/odp"},"keywords":["agent","developer","documentation"],"language":"en","localizations":["en"],"name":"ODP Developer Resources","odp_version":"1.0","operations":[{"authentication":"not-required","name":"get-offering"},{"authentication":"not-required","name":"list-offerings"}]}"#,
    )?;
    let collection = parse_collection(
        br#"{"description":"Guides and reference materials","id":"resources","name":"Resources","odp_version":"1.0"}"#,
    )?;
    let guide = parse_offering(
        br#"{"actions":[{"authentication":"not-required","description":"Download the plan","http":{"href":"/downloads/incident-plan.txt","method":"GET","response_content_types":["text/plain"]},"id":"download","rel":"download"}],"collection_ids":["resources"],"description":"A downloadable incident-response planning template.","id":"incident-plan","name":"Incident Response Plan","odp_version":"1.0","price":{"type":"free"}}"#,
    )?;
    let review = parse_offering(
        br#"{"description":"A one-time architecture review.","id":"architecture-review","name":"Architecture Review","odp_version":"1.0","price":{"amount":"500","currency":"USD","type":"starting_at"}}"#,
    )?;
    let catalog = StaticCatalog::new(StaticCatalogOptions {
        collections: vec![collection],
        offerings: vec![guide, review],
    })?;
    Ok(Service::new(document, Arc::new(catalog))?)
}
