# SEC API Rust SDK

Bootstrap async Rust client for SEC API factor data, filings, statements, ownership, and agent workflows.

## Representative workflows

- canonical entity resolution and search
- filing lookup, accession reads, and section reads
- all-statements bundle
- factor catalog, returns, history, valuations, exposures, decomposition, pairs, and custom discovery
- portfolio factor attribution, hedging, optimization, stress testing, and model factor analysis
- offerings, market calendar, and volatility signal utilities
- MCP info discovery

## Example

Methods return `Result<serde_json::Value, SecApiError>`: HTTP failures, non-2xx API responses (with parsed JSON body when possible), JSON decode errors, and invalid `x-api-key` header values are surfaced instead of being ignored.

```rust
use sec_api_sdk_rust::SecApiClient;

#[tokio::main]
async fn main() {
    let api_key = std::env::var("SECAPI_API_KEY")
        .ok();
    let client = SecApiClient::new(api_key);
    let entity = client.resolve_entity(&[("ticker", "AAPL")]).await.unwrap();
    let filing = client.latest_filing(&[("ticker", "AAPL"), ("form", "10-K")]).await.unwrap();
    let section = client.latest_section("item_1a", &[("ticker", "AAPL"), ("form", "10-K"), ("mode", "compact")]).await.unwrap();
    println!("{} {} {}", entity["name"], filing["id"], section["title"]);
}
```

## Factor Quickstart

Use `response_mode=compact` when you are feeding an agent, LLM, notebook, or UI card and want the smallest useful payload. Add `include=trust` when you need freshness, methodology, and materialization metadata for citations or launch checks.

```rust
use sec_api_sdk_rust::SecApiClient;
use serde_json::json;

#[tokio::main]
async fn main() {
    let client = SecApiClient::new(std::env::var("SECAPI_API_KEY").ok());

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


## Environment variables

| Variable | Description |
|---|---|
| `SECAPI_API_KEY` | Preferred SEC API key variable |
| `SECAPI_BASE_URL` | Preferred API base URL override |

## Release path

- crate metadata lives in `packages/sdk-rust/Cargo.toml`
- smoke command: `bun run smoke:sdk-rust`
- package verification command: `cargo test --examples` or `cargo run --example basic`
