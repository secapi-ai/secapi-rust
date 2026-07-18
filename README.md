# SEC API Rust SDK

Async Rust client for the SEC API.

## Install

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

This is a library crate; add it to an application rather than using `cargo install`.

## Request a filing

Create an API key at [secapi.ai/signup](https://secapi.ai/signup), then set it before running the program:

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

This calls `GET /v1/filings/latest?ticker=AAPL&form=10-K` and prints the JSON response for the latest matching filing. The result changes when a newer matching filing is available.

## Authentication and errors

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends it in the `x-api-key` header. Pass a key directly with `SecApiClient::new(Some("secapi_live_...".to_owned()))`. The client also reads `SECAPI_BEARER_TOKEN` and sends it as a bearer token; use `with_bearer_token` to set one explicitly.

Methods return `Result<serde_json::Value, SecApiError>`. For non-2xx responses, inspect `status()`, `code()`, `message()`, `request_id()`, and `body()` on `SecApiError`. Include the request ID when contacting support. By default, GET requests retry transport failures and HTTP `408`, `429`, `502`, `503`, and `504`; POST requests retry only HTTP `429`; DELETE requests do not retry.

For endpoints, pagination, and parameters, see the [API documentation](https://docs.secapi.ai). Report SDK issues or request help in the [Rust SDK issue tracker](https://github.com/secapi-ai/secapi-rust/issues). See [Cargo.toml](https://github.com/secapi-ai/secapi-rust/blob/main/Cargo.toml) for the declared Rust edition and dependencies.

## License

MIT
