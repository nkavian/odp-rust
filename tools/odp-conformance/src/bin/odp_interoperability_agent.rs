use odp_agent::ServiceClient;
use odp_core::Representation;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service_url = std::env::args()
        .nth(1)
        .ok_or("usage: odp-interoperability-agent SERVICE_URL")?;
    let client = ServiceClient::new(&service_url)?;
    let inspection = client.inspect().await?;
    if inspection.document.name.is_empty() {
        return Err("Service name is empty".into());
    }
    let offerings = client.list_offerings(Representation::Terse, 50).await?;
    let first = offerings
        .items
        .first()
        .ok_or("Service returned no Offerings")?;
    let details = client.get_offering_details(&first.id).await?;
    if details.offering.id != first.id || details.offering.name != first.name {
        return Err("full Offering does not match its listed summary".into());
    }
    if let Some(action) = details.actions.first() {
        let resolved = client.resolve_action(&first.id, &action.id).await?;
        if resolved.action.id != action.id {
            return Err("resolved Action identifier changed".into());
        }
    }
    println!("Rust Agent interoperates with the ODP Service");
    Ok(())
}
