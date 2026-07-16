# SEC API Rust SDK

`sec-api-sdk-rust` is an async Rust client for querying SEC API filings, companies, filing sections, and search endpoints.

## Add it to a Tokio project

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

This is a library crate, so use `cargo add` in an application rather than `cargo install sec-api-sdk-rust`, which has no default binary target to install.

## Get the latest filing

Get an API key at [secapi.ai/signup](https://secapi.ai/signup), then set it before running your program:

```bash
export SECAPI_API_KEY="secapi_live_..."
```

```rust
use sec_api_sdk_rust::{LatestFilingRequest, SecApiClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SecApiClient::new(None);
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await?;

    println!("{filing}");
    Ok(())
}
```

The request calls `GET /v1/filings/latest?ticker=AAPL&form=10-K` and prints the JSON response for the latest matching filing. Its contents can change when a newer filing is available.

## Authentication and errors

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends it in the `x-api-key` request header. To provide a key directly, use `SecApiClient::new(Some("secapi_live_...".to_owned()))`. The client also supports `SECAPI_BEARER_TOKEN` for bearer-token authentication.

Methods return `Result<serde_json::Value, SecApiError>`. For a non-2xx response, inspect `status()`, `code()`, `message()`, `request_id()`, and `body()` on `SecApiError`; include the request ID when contacting support. By default, GET and text GET requests retry transport failures and HTTP `408`, `429`, `502`, `503`, and `504`; POST requests retry only HTTP `429`; DELETE requests are not retried.

For other endpoints, pagination, and request parameters, see the [API documentation](https://docs.secapi.ai). Report SDK issues or request help in the [Rust SDK repository](https://github.com/secapi-ai/secapi-rust/issues). For declared edition and dependency compatibility, see [Cargo.toml](https://github.com/secapi-ai/secapi-rust/blob/main/Cargo.toml).

## License

MIT
