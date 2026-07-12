# SEC API Rust SDK

`sec-api-sdk-rust` is an async Rust client for SEC API. Use it to retrieve SEC filings and filing sections, company statements and ownership data, factor and portfolio analytics, and hosted MCP tool results.

## Install

Add the SDK and Tokio runtime to your application's `Cargo.toml`:

```toml
[dependencies]
sec-api-sdk-rust = "1.0.2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Set an API key before running your program:

```bash
export SECAPI_API_KEY="secapi_live_..."
```

## Smallest working example

This program fetches Apple's latest 10-K and prints its filing ID. `SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends requests to `https://api.secapi.ai` by default.

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SecApiClient::new(None);
    let filing = client
        .latest_filing(&[("ticker", "AAPL"), ("form", "10-K")])
        .await?;

    println!("{}", filing["id"]);
    Ok(())
}
```

## Common workflows

Use raw query pairs for direct access to API parameters, or typed request builders for entity resolution, latest filings, latest filing sections, and semantic search.

```rust
use sec_api_sdk_rust::{LatestSectionRequest, ResolveEntityRequest, SecApiClient};

let client = SecApiClient::new(None);

let entity = client
    .resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL"))
    .await?;

let risk_factors = client
    .latest_section_with(
        &LatestSectionRequest::new("item_1a")
            .ticker("AAPL")
            .form("10-K")
            .mode("compact"),
    )
    .await?;
```

The client also exposes grouped services through `entities()`, `filings()`, `sections()`, `search()`, and `factors()`. Cursor endpoints support `paginate_entities`, `paginate_filings`, and `paginate_sections`.

## Authentication and configuration

`SecApiClient::new(Some(api_key))` uses the supplied API key. With `None`, it reads `SECAPI_API_KEY`; `SECAPI_BEARER_TOKEN` supplies an optional bearer token. `OMNI_DATASTREAM_API_KEY` and `OMNI_DATASTREAM_BEARER_TOKEN` remain supported as compatibility fallbacks.

Requests use a 30-second timeout. Safe `GET` requests retry transient network failures and `408`, `429`, `502`, `503`, and `504` responses. POST helpers do not retry transport or 5xx failures, but they do retry `429` according to the retry configuration. Use `with_timeout`, `with_retry_config`, `without_retries`, `with_http_client`, or `with_base_url` when your application needs different transport behavior.

Methods that return an API response body use `Result<serde_json::Value, SecApiError>`; operations such as `delete_api_key` return `Result<(), SecApiError>`. For API responses outside the 2xx range, `SecApiError` provides `status()`, `request_id()`, `code()`, `message()`, and `body()`.

## Further reading

- [SEC API documentation](https://docs.secapi.ai)
- [SEC API developer resources](https://secapi.ai/developers)
- [Rust API reference](https://docs.rs/sec-api-sdk-rust)

## License

MIT
