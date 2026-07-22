# SEC API Rust SDK

`sec-api-sdk-rust` is an async Rust client for SEC API filings, statements, ownership data, factor data, and filing sections.

[Documentation](https://docs.secapi.ai) · [Get an API key](https://secapi.ai/signup) · [Support](https://github.com/secapi-ai/secapi-rust) · [Status](https://status.secapi.ai)

## Install and make a request

```toml
[dependencies]
sec-api-sdk-rust = "1.0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```bash
export SECAPI_API_KEY="secapi_live_..."
```

```rust
use sec_api_sdk_rust::{LatestFilingRequest, SecApiClient};

#[tokio::main]
async fn main() {
    let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await
        .unwrap();

    println!("{}", filing["accessionNumber"].as_str().unwrap());
    println!("{}", filing["filingUrl"].as_str().unwrap());
}
```

Run with `cargo run`. It prints the latest matching filing's accession number and SEC source URL; both can change after a new filing.

## Common requests

```rust
use sec_api_sdk_rust::{LatestSectionRequest, ResolveEntityRequest};

let company = client.resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL")).await?;
let section = client
    .latest_section_with(&LatestSectionRequest::new("item_1a").ticker("AAPL").form("10-K").mode("compact"))
    .await?;
```

Typed builders cover entity resolution, latest filings, latest sections, and semantic search. Flat methods accept query-pair slices; grouped services such as `client.filings()` and `client.sections()` provide discoverable equivalents.

## Special Situations

Use `list_situations`, `get_situation`, `situations_by_form`, `situation_filings`, `situation_summary`, `situations_feed`, `situations_calendar`, `situations_stats`, `situations_issues`, `export_situation`, `underwrite_situation`, and `watch_situations` for the authenticated paid Special Situations workflow. Use `embed_situations` and `embed_situation_export` for an anonymous, recent-only public projection. The [Special Situations workflow guide](https://docs.secapi.ai/special-situations-workflows) has concise examples and source-review guidance.

## Factor response modes

Use `response_mode=compact` when you want the smallest useful payload. Compact catalog responses still include readiness/proof summaries. Set `include=trust` only when you need the full trust/provenance envelope plus full methodology/materialization/revision/source-rights objects for citations or checks. For catalog/tool-discovery calls, start narrow with `category` and `limit`; the full trust envelope can be larger than a simple picker payload.

## Configuration and compatibility

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and `SECAPI_BEARER_TOKEN`; `OMNI_DATASTREAM_API_KEY` and `OMNI_DATASTREAM_BEARER_TOKEN` remain supported for compatibility. The default API version is `2026-03-19`. Use `with_base_url(...)` for a non-default origin.

Methods return `Result<serde_json::Value, SecApiError>`. For API failures, `SecApiError` exposes status, code, message, request ID, and response body. Include the request ID when opening an issue in the [Rust SDK repository](https://github.com/secapi-ai/secapi-rust). See the [API documentation](https://docs.secapi.ai) for retries, pagination, and complete endpoint coverage.

## License

MIT
