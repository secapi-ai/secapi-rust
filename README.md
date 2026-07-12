# SEC API Rust SDK

`sec-api-sdk-rust` is an asynchronous Rust client for the SEC API. Use it to retrieve SEC filings, filing sections, company data, and other API responses from a Rust application.

## Install

Add the SDK and a Tokio runtime:

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

The Cargo package is `sec-api-sdk-rust`; import it in Rust as `sec_api_sdk_rust`.

## Make a request

Store your API key outside source control and provide it at runtime:

```bash
export SECAPI_API_KEY="secapi_live_..."
```

This example gets the latest Apple 10-K and prints its filing ID:

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("SECAPI_API_KEY").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "SECAPI_API_KEY must be set",
        )
    })?;

    let client = SecApiClient::new(Some(api_key));
    let filing = client
        .latest_filing(&[("ticker", "AAPL"), ("form", "10-K")])
        .await?;

    println!("{}", filing["id"]);
    Ok(())
}
```

`latest_filing` sends a request to the API's latest-filings endpoint and returns the JSON response as `serde_json::Value`. SDK methods return `Result<_, SecApiError>`, so request, API, and JSON errors can be handled with Rust's standard error flow.

## Documentation and support

- [Get started with SEC API](https://docs.secapi.ai/getting-started)
- [API reference](https://docs.secapi.ai/api-reference)
- [Libraries and SDKs](https://docs.secapi.ai/libraries-and-sdks)
- [Email support](mailto:support@secapi.ai)
- [Report an issue](https://github.com/secapi-ai/secapi-rust/issues)

## Compatibility and status

The published crate is `1.0.2`, uses the Rust 2021 edition, and runs asynchronously on Tokio. The client reads `SECAPI_API_KEY` when constructed with `SecApiClient::new(None)`; passing the key explicitly, as above, makes the application's credential boundary clear.

MIT licensed.
