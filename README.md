# SEC API Rust SDK

An asynchronous Rust client for retrieving SEC filings and issuer data from [SEC API](https://secapi.ai).

Use it when a Rust service needs to resolve an issuer, retrieve a filing, or search SEC records without writing the HTTP plumbing itself. The SDK sends requests to `https://api.secapi.ai` by default and returns SEC API JSON as `serde_json::Value`, so response fields remain visible.

## Install

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

This is a library crate, so add it to an application rather than installing it as a command-line program.

## Make one request

Create an API key in the [SEC API dashboard](https://secapi.ai/app/api-keys), then make it available only to the server process that will use it:

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

    println!("{}", filing["accessionNumber"]);
    Ok(())
}
```

This calls `GET /v1/filings/latest?ticker=AAPL&form=10-K`. On success, it prints the accession number for the latest matching filing. The result can change after Apple files a newer annual report, so retain the accession number and filing URL when you need a reproducible record.

## Authentication

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends it in the `x-api-key` header. You can also pass an API key explicitly with `SecApiClient::new(Some("secapi_live_...".to_owned()))`. Do not put API keys in browser code, query strings, or logs.

The SDK requires a Tokio runtime. It uses `reqwest` with Rustls, defaults to a 30-second timeout, and retries safe `GET` requests after transient network failures and `408`, `429`, `502`, `503`, or `504` responses. Set `SECAPI_BASE_URL` only when you are intentionally targeting a non-production environment.

## Handle errors

Every method returns `Result<serde_json::Value, SecApiError>`. For a non-success response, inspect `status()`, `code()`, `message()`, `request_id()`, and `body()` on `SecApiError`. Include the request ID when you contact [support](https://secapi.ai/support).

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SecApiClient::new(None);

    match client.latest_filing(&[("ticker", "AAPL"), ("form", "10-K")]).await {
        Ok(filing) => println!("{filing}"),
        Err(error) => eprintln!(
            "SEC API request failed: status={:?}, request_id={:?}, message={:?}",
            error.status(),
            error.request_id(),
            error.message(),
        ),
    }

    Ok(())
}
```

## Next steps

- Read the [Rust SDK guide](https://docs.secapi.ai/rust-sdk) for request builders, retries, pagination, and configuration.
- Use the [API reference](https://docs.secapi.ai/api-reference) for endpoint parameters and response fields.
- Review [pricing](https://secapi.ai/pricing) before expanding a production workload.
- Report SDK defects in [GitHub Issues](https://github.com/secapi-ai/secapi-rust/issues) or get help through [SEC API support](https://secapi.ai/support).

## License

MIT
