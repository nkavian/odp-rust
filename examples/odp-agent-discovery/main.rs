mod mock_directory;

use odp_core::Representation;

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
    let directory = mock_directory::discover(&candidates).await;
    if directory.is_empty() {
        return Err("no configured ODP Services are reachable".into());
    }
    println!(
        "Mock directory contains {} reachable ODP Service(s).",
        directory.len()
    );
    for entry in directory {
        println!("\nService: {} ({})", entry.document.name, entry.origin);
        print_json("ODP Service Document", &entry.document)?;
        let offerings = entry
            .client
            .list_offerings(Representation::Terse, 50)
            .await?;
        print_json("Terse Offering list", &offerings)?;
        for offering in offerings.items {
            let full = entry.client.get_offering(&offering.id).await?;
            print_json(&format!("Full Offering {}", offering.id), &full)?;
        }
    }
    Ok(())
}

fn print_json<T: serde::Serialize>(label: &str, value: &T) -> Result<(), serde_json::Error> {
    println!("\n{label}:\n{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
