# SEC API Rust SDK

`sec-api-sdk-rust` is an async Rust client for the [SEC API](https://secapi.ai/developers). Use it in services, workers, and command-line programs to retrieve issuer identities, filings, filing sections, statements, and other SEC API data.

The client sends requests to `https://api.secapi.ai` by default. Response bodies are returned as `serde_json::Value`, so they retain the API response shape.

## Quickstart

Add the SDK and a Tokio runtime:

```bash
cargo add sec-api-sdk-rust
cargo add tokio --features macros,rt-multi-thread
```

Set an API key from your SEC API account in the environment:

```bash
export SECAPI_API_KEY="secapi_live_..."
```

Then resolve an issuer and request its latest annual filing:

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

The Cargo package is `sec-api-sdk-rust`; import it as `sec_api_sdk_rust`. The repository includes this workflow as [`examples/basic.rs`](examples/basic.rs), which you can run with `cargo run --example basic`.

## Authentication

`SecApiClient::new(None)` reads `SECAPI_API_KEY` and sends it in the `x-api-key` header. Pass `Some(api_key)` when your application loads credentials itself. Keep API keys in a server-side secret store or runtime environment; never commit them or expose them in browser code.

For endpoints that use a configured identity flow, set `SECAPI_BEARER_TOKEN` or call `with_bearer_token(...)`. `SECAPI_BASE_URL` overrides the default base URL. The legacy `OMNI_DATASTREAM_*` variables remain compatibility fallbacks; new integrations should use `SECAPI_*` names.

## Use the API surface

Typed builders cover issuer resolution, latest filings, latest filing sections, and semantic search. Other endpoints accept query pairs directly:

```rust
let filings = client
    .search_filings(&[("ticker", "AAPL"), ("form", "10-K")])
    .await?;
```

Use `paginate_entities`, `paginate_filings`, or `paginate_sections` for cursor-based results. Pass every cursor returned by the API back unchanged to the same route.

The client also exposes service groups through `entities()`, `filings()`, `sections()`, `search()`, and `factors()`. Refer to the [API reference](https://docs.secapi.ai/api-reference) for each endpoint's accepted parameters and response fields.

## Responses and errors

Endpoint responses have the type `Result<serde_json::Value, SecApiError>`. Fields can differ by endpoint and response view, so treat the endpoint reference as the field contract. For filing-derived data, preserve `accessionNumber`, `filingDate`, `filingUrl`, and the request ID when present; a latest-filing request can change after a new filing arrives.

For failures, `SecApiError` distinguishes transport errors, invalid credentials, JSON decode failures, and non-2xx API responses. API errors expose `status()`, `code()`, `message()`, `request_id()`, and `body()`. Log the request ID, not credentials.

## Timeouts and retries

The default request timeout is 30 seconds. The client retries safe `GET` requests up to two times after transport failures and HTTP `408`, `429`, `502`, `503`, or `504`, with bounded backoff that honors `Retry-After`. POST helpers retry only `429`.

Use `with_timeout`, `with_retry_config`, `without_retries`, or `with_http_client` to set your application's transport policy. Keep retry ownership in one layer, reduce concurrency after `429`, and retry a write only when it is safe to repeat.

## Documentation

- [Rust SDK guide](https://docs.secapi.ai/rust-sdk)
- [API overview](https://docs.secapi.ai/api-overview)
- [API reference](https://docs.secapi.ai/api-reference)
- [Rust API reference](https://docs.rs/sec-api-sdk-rust)
- [Report an issue](https://github.com/secapi-ai/secapi-rust/issues)

## License

MIT
