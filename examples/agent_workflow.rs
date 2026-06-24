use sec_api_sdk_rust::{LatestFilingRequest, ResolveEntityRequest, ResponseView, SecApiClient};
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = SecApiClient::new(None);

    let entity = client
        .resolve_entity_with(
            &ResolveEntityRequest::new()
                .ticker("AAPL")
                .view(ResponseView::Agent),
        )
        .await
        .unwrap();
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await
        .unwrap();
    let accession_number = filing
        .get("accessionNumber")
        .or_else(|| filing.get("accession_number"))
        .and_then(|value| value.as_str())
        .expect("latest filing response did not include an accession number");
    let section = client
        .filing_section_by_accession(
            accession_number,
            "item_1a",
            &[("ticker", "AAPL"), ("mode", "compact")],
        )
        .await
        .unwrap();
    let section_accession_number = section
        .get("accessionNumber")
        .or_else(|| section.get("accession_number"))
        .or_else(|| section.get("accession"))
        .and_then(|value| value.as_str())
        .unwrap_or(accession_number);

    println!(
        "{}",
        json!({
            "object": "secapi_sdk_agent_workflow",
            "sdk": "rust",
            "workflow": {
                "ticker": "AAPL",
                "form": "10-K",
                "sectionKey": "item_1a",
                "mode": "compact",
            },
            "entity": {
                "name": entity.get("name"),
                "ticker": entity.get("ticker"),
                "cik": entity.get("cik"),
            },
            "filing": {
                "id": filing.get("id"),
                "accessionNumber": accession_number,
                "form": filing.get("form"),
                "filingDate": filing.get("filingDate"),
            },
            "section": {
                "title": section.get("title"),
                "key": section.get("key").or_else(|| section.get("section_key")),
                "mode": "compact",
                "accessionNumber": section_accession_number,
                "contentLength": section
                    .get("contentMd")
                    .or_else(|| section.get("snippet"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.len())
                    .unwrap_or(0),
            },
        })
    );
}
