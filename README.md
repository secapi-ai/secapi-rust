# SEC API Rust SDK

Bootstrap async Rust client for SEC API.

## Representative workflows

- canonical entity resolution and search
- filing lookup, accession reads, and section reads
- all-statements bundle
- offerings, market calendar, and volatility signal utilities
- MCP info discovery

## Example

Methods return `Result<serde_json::Value, OmniDatastreamError>`: HTTP failures, non-2xx API responses (with parsed JSON body when possible), JSON decode errors, and invalid `x-api-key` header values are surfaced instead of being ignored.

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() {
    let api_key = std::env::var("SECAPI_API_KEY")
        .or_else(|_| std::env::var("OMNI_DATASTREAM_API_KEY"))
        .ok();
    let client = SecApiClient::new(api_key);
    let entity = client.resolve_entity(&[("ticker", "AAPL")]).await.unwrap();
    let filing = client.latest_filing(&[("ticker", "AAPL"), ("form", "10-K")]).await.unwrap();
    let section = client.latest_section("item_1a", &[("ticker", "AAPL"), ("form", "10-K"), ("mode", "compact")]).await.unwrap();
    println!("{} {} {}", entity["name"], filing["id"], section["title"]);
}
```

`OmniDatastreamClient` remains available for existing integrations.

## Environment variables

| Variable | Description |
|---|---|
| `SECAPI_API_KEY` | Preferred SEC API key variable |
| `OMNI_DATASTREAM_API_KEY` | Legacy fallback for existing integrations |
| `SECAPI_BASE_URL` | Preferred API base URL override |
| `OMNI_DATASTREAM_BASE_URL` | Legacy base URL override fallback |

## Release path

- crate metadata lives in `packages/sdk-rust/Cargo.toml`
- smoke command: `bun run smoke:sdk-rust`
- package verification command: `cargo test --examples` or `cargo run --example basic`
