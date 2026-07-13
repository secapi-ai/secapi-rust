use sec_api_sdk_rust::{LatestFilingRequest, ResolveEntityRequest, SecApiClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SecApiClient::new(None);
    let entity = client
        .resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL"))
        .await?;
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await?;

    println!("entity: {entity}");
    println!("latest 10-K: {filing}");
    Ok(())
}
