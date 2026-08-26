mod mock_directory;

use odp_agent::FederatedSearchRequest;
use odp_core::{OfferingSearchRequest, Operation, Representation};
use odp_directory::SearchRequest;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let candidates = if arguments.is_empty() {
        (4101..=4104)
            .map(|port| format!("http://127.0.0.1:{port}"))
            .collect()
    } else {
        arguments
    };
    let directory = mock_directory::discover(&candidates).await?;
    if directory.entries.is_empty() {
        return Err("no configured ODP Services are reachable".into());
    }
    println!(
        "Mock directory contains {} reachable ODP Service(s).",
        directory.entries.len()
    );
    let events = directory
        .agent
        .search_offerings_across_services(&FederatedSearchRequest {
            concurrency: 4,
            max_offerings_per_service: 50,
            max_services: 50,
            offerings: OfferingSearchRequest::default(),
            services: SearchRequest::default(),
        })
        .await?;
    println!("\nFederated Offering discovery:");
    for event in events {
        if let Some(offering) = event.offering {
            println!(
                "- {}: {} ({})",
                event.service.name, offering.name, offering.id
            );
        } else if let Some(issue) = event.issue {
            println!("- {}: unavailable ({issue})", event.service.name);
        }
    }

    for entry in directory.entries {
        println!("\nService: {} ({})", entry.document.name, entry.origin);
        print_json("ODP Service Document", &entry.document)?;
        if entry
            .document
            .operations
            .iter()
            .any(|descriptor| descriptor.name == Operation::ListCollections)
        {
            let collections = entry
                .client
                .list_collections(Representation::Terse, 50)
                .await?;
            print_json("Terse Collection list", &collections)?;
        }
        let offerings = entry
            .client
            .list_offerings(Representation::Terse, 50)
            .await?;
        print_json("Terse Offering list", &offerings)?;
        for offering in offerings.items {
            let details = entry.client.get_offering_details(&offering.id).await?;
            let action_ids = details
                .actions
                .iter()
                .map(|action| action.id.clone())
                .collect::<Vec<_>>();
            print_json(&format!("Full Offering {}", offering.id), &details.offering)?;
            if let Some(schema) = &details.attribute_schema {
                print_json("Attribute Schema", schema)?;
            }
            for issue in &details.issues {
                println!("Offering issue: {}", issue.message);
            }
            for action in &details.actions {
                println!("Available Action: {} ({:?})", action.id, action.rel);
            }
            for action_id in action_ids {
                let action = entry
                    .client
                    .resolve_action(&offering.id, &action_id)
                    .await?;
                if let Some(http) = action.action.http {
                    println!("Resolved Action {action_id}: {} {}", http.method, http.url);
                } else if let Some(openapi) = action.action.openapi {
                    println!(
                        "Resolved Action {action_id}: OpenAPI operation {} in {}",
                        openapi.operation_id, openapi.url
                    );
                }
            }
        }
    }
    Ok(())
}

fn print_json<T: serde::Serialize>(label: &str, value: &T) -> Result<(), serde_json::Error> {
    println!("\n{label}:\n{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
