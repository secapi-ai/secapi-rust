# SEC API Rust SDK

Bootstrap async Rust client for SEC API factor data, filings, statements, ownership, and agent workflows.

## Representative workflows

- canonical entity resolution and search
- filing lookup, accession reads, and section reads
- all-statements bundle
- factor catalog, returns, history, valuations, exposures, decomposition, pairs, and custom discovery
- portfolio factor attribution, hedging, optimization, stress testing, and model factor analysis
- offerings, market calendar, and volatility signal utilities
- MCP info discovery and hosted tool calls

## Example

Methods return `Result<serde_json::Value, SecApiError>`: HTTP failures, non-2xx API responses (with parsed JSON body when possible), JSON decode errors, and invalid auth header values are surfaced instead of being ignored. For non-2xx API responses, use `error.status()`, `error.request_id()`, `error.code()`, `error.message()`, and `error.body()` to log support-ready diagnostics without parsing the response body yourself.

The default HTTP client uses a 30-second request timeout and retries safe read requests (`GET`) on transient network errors, `408`, `429`, `502`, `503`, and `504` responses. Mutating requests are not retried automatically. Requests include `Accept: application/json` and a `User-Agent` like `secapi-rust/1.0.1` so support and server logs can identify SDK traffic. Use `with_timeout(...)` for stricter service budgets, `with_retry_config(...)` or `without_retries()` for custom retry ownership, and `with_http_client(...)` to provide a fully customized `reqwest::Client` for proxies, custom TLS, or shared transport settings.

`SecApiClient::new(None)` reads `SECAPI_API_KEY` / `OMNI_DATASTREAM_API_KEY` and `SECAPI_BEARER_TOKEN` / `OMNI_DATASTREAM_BEARER_TOKEN` from the environment. Dashboard/account workflows can also set a bearer token explicitly with `with_bearer_token(...)`.

```rust
use sec_api_sdk_rust::{
    LatestFilingRequest, LatestSectionRequest, ResolveEntityRequest, SecApiClient,
};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("SECAPI_API_KEY")
        .ok();
    let client = SecApiClient::new(api_key);
    let entity = client
        .resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL"))
        .await
        .unwrap();
    let filing = client
        .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
        .await
        .unwrap();
    let section = client
        .latest_section_with(&LatestSectionRequest::new("item_1a").ticker("AAPL").form("10-K").mode("compact"))
        .await
        .unwrap();
    println!("{} {} {}", entity["name"], filing["id"], section["title"]);
}
```

### Live agent workflow example

For a production-backed copy/paste path, run the focused example that resolves
an entity, fetches the latest 10-K, and extracts Item 1A in compact mode:

```bash
export SECAPI_API_KEY="secapi_live_..."
cd packages/sdk-rust
cargo run --example agent_workflow
```

From the monorepo root, `bun run smoke:sdk-examples` runs the matching
JavaScript, Python, Go, and Rust examples and asserts that each returns entity,
filing, and compact-section metadata.

## Typed request builders

Use typed request builders when you want editor completion and canonical query
names without giving up the existing raw `&[(&str, &str)]` methods:

```rust
use sec_api_sdk_rust::{
    LatestFilingRequest, LatestSectionRequest, ResolveEntityRequest, ResponseView,
    SecApiClient, SemanticSearchMode, SemanticSearchRequest,
};

let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());

let entity = client
    .resolve_entity_with(&ResolveEntityRequest::new().ticker("AAPL").view(ResponseView::Agent))
    .await?;

let filing = client
    .latest_filing_with(&LatestFilingRequest::new().ticker("AAPL").form("10-K"))
    .await?;

let section = client
    .latest_section_with(&LatestSectionRequest::new("item_1a").ticker("AAPL").form("10-K").mode("compact"))
    .await?;

let semantic = client
    .semantic_search_with(
        &SemanticSearchRequest::new("risk factors")
            .ticker("AAPL")
            .form("10-K")
            .mode(SemanticSearchMode::Hybrid)
            .limit("3")
            .view(ResponseView::Agent),
    )
    .await?;
```

Builders are available for entity resolution, latest filings, latest sections,
and semantic search. Each builder has `extra(key, value)` for newer or less
common query params while preserving the exact REST wire contract.

## Grouped services

Flat methods remain the complete SDK surface, and common workflows are also
available through borrowed grouped services for easier discovery in editors,
REPLs, and agent tool planners:

```rust
let filing = client
    .filings()
    .latest(&[("ticker", "AAPL"), ("form", "10-K")])
    .await?;

let section = client
    .sections()
    .latest("item_1a", &[("ticker", "AAPL"), ("form", "10-K"), ("mode", "compact")])
    .await?;

let semantic = client
    .search()
    .semantic(&[
        ("q", "supply chain risk"),
        ("ticker", "AAPL"),
        ("mode", "hybrid"),
        ("view", "agent"),
    ])
    .await?;

let history = client
    .factors()
    .history("VALUE", &[("range", "1y"), ("response_mode", "compact")])
    .await?;
```

Start with `client.entities()`, `client.filings()`, `client.sections()`,
`client.search()`, and `client.factors()` when exploring. The grouped services
borrow the client and delegate to the flat methods, so auth, retries, timeouts,
and custom HTTP clients behave exactly the same.

## Auto-pagination

Cursor endpoints can be consumed with async pull iterators instead of
hand-writing `nextCursor` loops:

```rust
use sec_api_sdk_rust::{PaginationOptions, SecApiClient};

let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());
let mut filings = client.paginate_filings_with_options(
    &[("ticker", "AAPL"), ("form", "10-K"), ("limit", "100")],
    PaginationOptions::default().max_pages(5).max_items(200),
);

while let Some(filing) = filings.next().await? {
    println!("{}", filing["accessionNumber"]);
}
```

Built-in helpers cover `paginate_filings`, `paginate_sections`, and
`paginate_entities`. Use `paginate("/v1/...", params, options)` for less common
cursor endpoints.

## API errors

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() {
    let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());

    match client.latest_filing(&[("ticker", "AAPL"), ("form", "10-K")]).await {
        Ok(filing) => println!("{}", filing["id"]),
        Err(error) => {
            eprintln!(
                "SEC API failed: status={:?} code={:?} request_id={:?} message={:?}",
                error.status(),
                error.code(),
                error.request_id(),
                error.message()
            );
            if let Some(body) = error.body() {
                eprintln!("raw error body: {body}");
            }
        }
    }
}
```

## Factor Quickstart

Use `response_mode=compact` when you are feeding an agent, LLM, notebook, or UI card and want the smallest useful payload. Compact catalog responses still include readiness/proof summaries. Add `include=trust` only when you need the full trust/provenance envelope plus full methodology/materialization/revision/source-rights objects for citations or checks. For catalog/tool-discovery calls, start narrow with `category` plus `limit` before requesting trust metadata; the full trust envelope can be larger than a simple picker payload.

```rust
use sec_api_sdk_rust::SecApiClient;
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());

    let catalog = client.factor_catalog(&[
        ("category", "style"),
        ("limit", "25"),
        ("response_mode", "compact"),
        ("include", "trust"),
    ]).await.unwrap();
    println!("{}", catalog["data"]);

    let history = client.factor_history(
        "VALUE",
        &[("range", "1y"), ("response_mode", "compact"), ("include", "trust,series")],
    ).await.unwrap();
    println!("{} {}", history["factorKey"], history["dataAsOf"]);

    let valuations = client.factor_valuations(&[
        ("keys", "VALUE,QUALITY,MOMENTUM"),
        ("side", "all"),
        ("sort", "opportunity_score"),
        ("response_mode", "compact"),
        ("include", "trust"),
        ("limit", "25"),
    ]).await.unwrap();
    println!("{}", valuations["data"]);

    let dashboard = client.factor_dashboard(&[
        ("country", "US"),
        ("category", "style"),
        ("ticker", "AAPL"),
        ("response_mode", "compact"),
    ]).await.unwrap();
    println!("{}", dashboard["data"]);

    let extreme_moves = client.factor_extreme_moves(&[
        ("category", "style"),
        ("window", "1d"),
        ("min_z_score", "2"),
        ("response_mode", "compact"),
    ]).await.unwrap();
    println!("{}", extreme_moves["data"]);

    let extreme_pairs = client.factor_extreme_pairs(&[
        ("category", "style"),
        ("window", "1m"),
        ("min_z_score", "1"),
        ("response_mode", "compact"),
    ]).await.unwrap();
    println!("{}", extreme_pairs["data"]);

    let holdings = json!([
        { "symbol": "AAPL", "weight": 0.4 },
        { "symbol": "MSFT", "weight": 0.35 },
        { "symbol": "NVDA", "weight": 0.25 }
    ]);

    let attribution = client.portfolio_attribution_with_params(
        &json!({
            "holdings": holdings.clone(),
            "window": "1y",
            "frequency": "monthly"
        }),
        &[("response_mode", "compact"), ("include", "trust")],
    ).await.unwrap();
    println!("{}", attribution["portfolio"]);

    let hedge = client.portfolio_hedge_with_params(
        &json!({
            "holdings": holdings.clone(),
            "objective": "factor_neutral",
            "constraints": { "maxHedges": 5 }
        }),
        &[("response_mode", "compact"), ("include", "trust")],
    ).await.unwrap();
    println!("{}", hedge["residualExposure"]);

    let optimized = client.portfolio_optimize_with_params(
        &json!({
            "holdings": holdings.clone(),
            "objective": "regime_aware",
            "constraints": { "longOnly": true, "maxPositionWeight": 0.35 }
        }),
        &[("response_mode", "compact"), ("include", "trust")],
    ).await.unwrap();
    println!("{}", optimized["optimizationNotes"]);

    let model_analysis = client.model_factor_analysis_with_params(
        &json!({
            "model": { "id": "growth-core", "label": "Growth Core" },
            "holdings": holdings.clone(),
            "include": { "attribution": true, "hedge": true, "optimizer": true }
        }),
        &[("response_mode", "compact"), ("include", "trust")],
    ).await.unwrap();
    println!("{}", model_analysis["summaryMd"]);
}
```

Factor and portfolio helpers include `factor_catalog`, `factor_returns`, `factor_history`, `factor_sparklines`, `factor_dashboard`, `factor_screen`, `factor_extreme_moves`, `factor_extreme_pairs`, `factor_valuations`, `factor_valuation_stocks`, `factor_exposures`, `factor_decomposition`, `factor_related_stocks`, `factor_similarity_pack`, `factor_pairs`, `factor_pair_history`, `factor_bulk_download`, `factor_custom_with_params`, `portfolio_analyze_with_params`, `portfolio_attribution_with_params`, `portfolio_hedge_with_params`, `portfolio_optimize_with_params`, `portfolio_stress_test_with_params`, `model_portfolio_factor_view`, and `model_factor_analysis_with_params`.

## MCP Tool Calls

Use `call_mcp_tool` to invoke hosted MCP tools without hand-writing the JSON-RPC envelope:

```rust
use serde_json::json;

let result = client.call_mcp_tool(
    "filings.latest",
    &[("ticker", json!("AAPL")), ("form", json!("10-K"))],
    Some("agent-request-1"),
).await.unwrap();
println!("{result}");
```

## HTTP configuration

```rust
use sec_api_sdk_rust::SecApiClient;
use std::time::Duration;

let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok())
    .with_timeout(Duration::from_secs(10));
```

## Retries

Safe read requests retry transient failures by default: network errors, `408`, `429`, `502`, `503`, and `504`. `Retry-After` is honored and clamped to the configured maximum backoff. Mutating requests, including hosted MCP tool calls, are not retried automatically.

```rust
use sec_api_sdk_rust::{RetryConfig, SecApiClient};
use std::time::Duration;

let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok())
    .with_retry_config(RetryConfig {
        max_retries: 2,
        initial_backoff: Duration::from_millis(200),
        max_backoff: Duration::from_secs(2),
    });
```

Disable SDK retries when your service already owns retries:

```rust
let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok())
    .without_retries();
```

For advanced transports, pass your own `reqwest::Client`:

This requires a compatible direct `reqwest` dependency in your application.

```rust
use sec_api_sdk_rust::SecApiClient;

let http = reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(15))
    .build()
    .unwrap();

let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok())
    .with_http_client(http);
```

If you pass a custom `reqwest::Client`, configure its timeout on the builder you provide. Chained setters are last-write-wins, so calling `with_timeout(...)` after `with_http_client(...)` replaces the custom client.

## Environment variables

| Variable | Description |
|---|---|
| `SECAPI_API_KEY` | Preferred SEC API key variable |
| `SECAPI_BEARER_TOKEN` | Optional OAuth bearer token env var |
| `SECAPI_BASE_URL` | Preferred API base URL override |
| `SECAPI_API_BASE_URL` | API base URL override alias |
| `OMNI_DATASTREAM_API_KEY` | Compatibility API key variable |
| `OMNI_DATASTREAM_BEARER_TOKEN` | Compatibility bearer token env var |
| `OMNI_DATASTREAM_BASE_URL` | Compatibility base URL override |
| `OMNI_DATASTREAM_API_BASE_URL` | Compatibility base URL override alias |

## Release path

- crate metadata lives in `packages/sdk-rust/Cargo.toml`
- smoke command: `bun run smoke:sdk-rust`
- package verification command: `cargo test --examples` or `cargo run --example basic`
