# SEC API Rust SDK

`sec-api-sdk-rust` is an asynchronous client for SEC API's public REST endpoints. Use it from a Rust service, worker, or command-line program to retrieve company identities, filings and filing sections, statements, ownership data, market data, and other SEC API resources.

The SDK sends requests to `https://api.secapi.ai` by default and returns endpoint payloads as `serde_json::Value`, preserving the API response shape rather than translating it into crate-specific models.

## Install

Add the SDK and a Tokio runtime to your application:

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

The Cargo package is `sec-api-sdk-rust`; the Rust import is `sec_api_sdk_rust`.

## Authenticate

Create an API key in your SEC API account, store it in your runtime secret store, and expose it to the process:

```bash
export SECAPI_API_KEY="secapi_live_..."
```

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends it as the `x-api-key` header. Pass `Some(api_key)` when your application owns credential loading explicitly. API keys are for trusted server-side code; do not put one in a browser bundle or commit it to source control.

## Make a request

This complete async program resolves Apple, fetches its latest 10-K, and prints both JSON responses:

```rust
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
```

Run the repository example with `cargo run --example basic`. It uses the same `SECAPI_API_KEY` environment variable and production default base URL.

## Responses

Methods returning response bodies have the type `Result<serde_json::Value, SecApiError>`. Fields vary by endpoint and response view, so use the endpoint reference as the authoritative schema and access optional fields deliberately. For a filing-derived response, preserve `accessionNumber`, `filingDate`, and `filingUrl` when present; they identify the underlying SEC filing. A current/latest request is not a stable fixture and can return a different accession after a new filing.

For example, an entity lookup includes identity fields such as:

```json
{
  "ticker": "AAPL",
  "cik": "0000320193"
}
```

List endpoints can return an array under `data`, `items`, `results`, `sections`, or `filings`. Use `paginate_entities`, `paginate_filings`, or `paginate_sections` for the SDK's cursor iterator, and retain each returned cursor unchanged.

## Errors, retries, and timeouts

`SecApiError` distinguishes transport failures (`Request`), invalid credential-header values, invalid JSON (`JsonDecode`), and non-2xx API responses (`Api`). For API responses, inspect `status()`, `code()`, `message()`, `request_id()`, and `body()`; retain the request ID in logs and support requests, but never log credentials.

The default HTTP timeout is 30 seconds. The SDK retries safe `GET` requests up to two times after transport failures and HTTP `408`, `429`, `502`, `503`, or `504`, using a 200 ms initial backoff capped at two seconds and honoring `Retry-After` within that cap. POST helpers retry only `429`; they do not retry transport or 5xx failures. Keep retry ownership in one layer, reduce concurrency on `429`, and only retry writes when your application can safely repeat them.

Use `with_timeout`, `with_retry_config`, `without_retries`, or `with_http_client` to set your application's transport policy. When injecting a `reqwest::Client`, configure its timeout on that client; a later `with_timeout(...)` replaces the injected client.

## Configuration

`SECAPI_BASE_URL` overrides the default base URL for controlled environments. `SECAPI_BEARER_TOKEN` or `with_bearer_token(...)` configures a bearer credential for endpoints that explicitly require that identity flow. The legacy `OMNI_DATASTREAM_*` environment variables remain compatibility fallbacks; new integrations should use the `SECAPI_*` names.

Use typed builders for entity resolution, latest filings, latest filing sections, and semantic search. Other endpoints accept query pairs directly, for example `client.search_filings(&[("ticker", "AAPL")]).await?`.

## Documentation and support

- [Rust SDK guide](https://docs.secapi.ai/rust-sdk)
- [Build a first SEC API integration](https://docs.secapi.ai/api-overview)
- [API reference](https://docs.secapi.ai/api-reference)
- [Libraries and SDKs](https://docs.secapi.ai/libraries-and-sdks)
- [Rust API reference](https://docs.rs/sec-api-sdk-rust)
- [Email support](mailto:support@secapi.ai)
- [Report an issue](https://github.com/secapi-ai/secapi-rust/issues)

## License

MIT
