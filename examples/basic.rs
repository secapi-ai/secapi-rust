use sec_api_sdk_rust::{
    LatestFilingRequest, LatestSectionRequest, ResolveEntityRequest, SecApiClient,
};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("SECAPI_API_KEY").ok();
    // Uses https://api.secapi.ai by default; SECAPI_BASE_URL may override it.
    let client = SecApiClient::new(api_key);
    let entity = client
        .resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL"))
        .await
        .unwrap();
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await
        .unwrap();
    let section = client
        .latest_section_with(
            &LatestSectionRequest::new("item_1a")
                .ticker("AAPL")
                .form("10-K")
                .mode("compact"),
        )
        .await
        .unwrap();

    // Dilution lists return 200 even when empty;
    // `coverage` returns 200 with a rollup. Both safe under any seed state.
    let dilution_events = client
        .dilution_events(&[("ticker", "AAPL"), ("limit", "3")])
        .await
        .unwrap();
    let dilution_ratings = client.dilution_ratings(&[("limit", "3")]).await.unwrap();
    let dilution_coverage = client.dilution_coverage(&[]).await.unwrap();

    println!(
        "{} {} {} {} {} {}",
        entity["name"],
        filing["id"],
        section["title"],
        dilution_events["object"],
        dilution_ratings["object"],
        dilution_coverage["object"],
    );
}
