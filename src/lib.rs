use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::Value;
use std::fmt;

const DEFAULT_API_VERSION: &str = "2026-03-19";

/// `?view=` response mode. Mirrors the canonical `ResponseView` union in
/// SEC API contracts. Pass the `.as_str()` value as the "view"
/// query param on endpoints that support agent mode;
/// agent mode returns a strictly smaller, essentials+citation-pointers shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseView {
    Default,
    Compact,
    Agent,
}

impl ResponseView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Compact => "compact",
            Self::Agent => "agent",
        }
    }
}

/// Errors returned by [`SecApiClient`] (HTTP failures, non-2xx API responses, JSON decode).
#[derive(Debug)]
pub enum SecApiError {
    Request(reqwest::Error),
    InvalidApiKeyHeader,
    JsonDecode(serde_json::Error),
    Api { status: u16, body: Value },
}

impl fmt::Display for SecApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "HTTP client error: {e}"),
            Self::InvalidApiKeyHeader => {
                write!(f, "API key contains characters that cannot be sent in the x-api-key header")
            }
            Self::JsonDecode(e) => write!(f, "invalid JSON response: {e}"),
            Self::Api { status, body } => write!(f, "API error HTTP {status}: {body}"),
        }
    }
}

impl std::error::Error for SecApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(e) => Some(e),
            Self::JsonDecode(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for SecApiError {
    fn from(value: reqwest::Error) -> Self {
        Self::Request(value)
    }
}

impl From<serde_json::Error> for SecApiError {
    fn from(value: serde_json::Error) -> Self {
        Self::JsonDecode(value)
    }
}

pub struct SecApiClient {
    api_key: Option<String>,
    base_url: String,
    api_version: String,
    http: reqwest::Client,
}

impl SecApiClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key,
            base_url: std::env::var("SECAPI_BASE_URL")
                .or_else(|_| std::env::var("SECAPI_API_BASE_URL"))
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "https://api.secapi.ai".to_string()),
            api_version: DEFAULT_API_VERSION.to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    fn headers(&self) -> Result<HeaderMap, SecApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let version_header = HeaderValue::from_str(&self.api_version).unwrap_or_else(|_| {
            HeaderValue::from_static(DEFAULT_API_VERSION)
        });
        headers.insert("secapi-version", version_header);
        if let Some(api_key) = &self.api_key {
            let value = HeaderValue::from_str(api_key).map_err(|_| SecApiError::InvalidApiKeyHeader)?;
            headers.insert("x-api-key", value);
        }
        Ok(headers)
    }

    async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let response = self
            .http
            .get(url)
            .headers(self.headers()?)
            .query(params)
            .send()
            .await?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text).map_err(SecApiError::JsonDecode);
        }
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Err(SecApiError::Api { status, body })
    }

    pub async fn health(&self) -> Result<Value, SecApiError> {
        self.get("/healthz", &[]).await
    }

    pub async fn me(&self) -> Result<Value, SecApiError> {
        self.get("/v1/me", &[]).await
    }

    pub async fn resolve_entity(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/entities/resolve", params).await
    }

    pub async fn search_entities(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/entities", params).await
    }

    pub async fn search_filings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/filings", params).await
    }

    pub async fn search_sections(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/sections/search", params).await
    }

    pub async fn search_fulltext(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/search/fulltext", params).await
    }

    pub async fn semantic_search(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/search/semantic", params).await
    }

    pub async fn latest_filing(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/filings/latest", params).await
    }

    pub async fn latest_section(&self, section_key: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/filings/latest/sections/{section_key}"), params).await
    }

    pub async fn filing_by_accession(&self, accession_number: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/filings/{accession_number}"), params).await
    }

    pub async fn filing_section_by_accession(
        &self,
        accession_number: &str,
        section_key: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/filings/{accession_number}/sections/{section_key}"), params).await
    }

    pub async fn all_statements(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/statements/all", params).await
    }

    pub async fn company_income_statements(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/income-statements", params).await
    }

    pub async fn company_balance_sheets(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/balance-sheets", params).await
    }

    pub async fn company_cash_flow_statements(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/cash-flow-statements", params).await
    }

    pub async fn company_financials(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/financials", params).await
    }

    pub async fn company_ratios(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/ratios", params).await
    }

    pub async fn company_resolve(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/resolve", params).await
    }

    pub async fn company_search(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/search", params).await
    }

    pub async fn offerings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/offerings", params).await
    }

    pub async fn market_calendar(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/calendar", params).await
    }

    pub async fn market_estimates(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/estimates", params).await
    }

    pub async fn market_snapshots(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/snapshots", params).await
    }

    pub async fn market_bars(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/bars", params).await
    }

    pub async fn market_corporate_actions(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/corporate-actions", params).await
    }

    pub async fn market_reference(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/market/reference", params).await
    }

    pub async fn news_stories(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/news/stories", params).await
    }

    pub async fn macro_indicators(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/indicators", params).await
    }

    pub async fn macro_releases(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/releases", params).await
    }

    pub async fn macro_calendar(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/calendar", params).await
    }

    pub async fn macro_forecasts(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/forecasts", params).await
    }

    pub async fn macro_high_signal_pack(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/high-signal-pack", params).await
    }

    pub async fn macro_regimes(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/regimes", params).await
    }

    pub async fn factor_catalog(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/catalog", params).await
    }

    pub async fn factor_returns(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/returns", params).await
    }

    pub async fn factor_history(&self, factor_key: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/factors/history/{}", urlencoding::encode(factor_key)), params).await
    }

    pub async fn factor_sparklines(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/sparklines", params).await
    }

    pub async fn factor_returns_intraday(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/returns/intraday", params).await
    }

    pub async fn factor_dashboard(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/dashboard", params).await
    }

    pub async fn factor_regime_performance(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/regime-performance", params).await
    }

    pub async fn factor_correlations(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/correlations", params).await
    }

    pub async fn factor_screen(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/screen", params).await
    }

    pub async fn factor_extreme_moves(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/extreme-moves", params).await
    }

    pub async fn factor_extreme_pairs(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/extreme-pairs", params).await
    }

    pub async fn factor_valuations(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/valuations", params).await
    }

    pub async fn factor_valuation_stocks(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/valuations/stocks", params).await
    }

    pub async fn factor_exposures(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/exposures", params).await
    }

    pub async fn factor_decomposition(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/decomposition", params).await
    }

    pub async fn factor_related_stocks(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/related-stocks", params).await
    }

    pub async fn factor_similarity_pack(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/similarity-pack", params).await
    }

    pub async fn factor_pairs(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/pairs", params).await
    }

    pub async fn factor_pair_history(&self, f1: &str, f2: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(
            &format!(
                "/v1/factors/pair-history/{}/{}",
                urlencoding::encode(f1),
                urlencoding::encode(f2),
            ),
            params,
        ).await
    }

    pub async fn factor_bulk_download(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/factors/bulk-download", params).await
    }

    pub async fn factor_custom(&self, body: &Value) -> Result<Value, SecApiError> {
        self.factor_custom_with_params(body, &[]).await
    }

    pub async fn factor_custom_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/factors/custom", body, params).await
    }

    pub async fn stock_loadings(&self, ticker: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/stocks/{}/loadings", ticker), params).await
    }

    pub async fn portfolio_analyze(&self, body: &Value) -> Result<Value, SecApiError> {
        self.portfolio_analyze_with_params(body, &[]).await
    }

    pub async fn portfolio_analyze_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/portfolio/analyze", body, params).await
    }

    pub async fn portfolio_attribution(&self, body: &Value) -> Result<Value, SecApiError> {
        self.portfolio_attribution_with_params(body, &[]).await
    }

    pub async fn portfolio_attribution_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/portfolio/attribution", body, params).await
    }

    pub async fn model_portfolio_factor_view(&self, portfolio_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/model-portfolios/{}/factor-view", portfolio_id), params).await
    }

    pub async fn model_factor_analysis(&self, body: &Value) -> Result<Value, SecApiError> {
        self.model_factor_analysis_with_params(body, &[]).await
    }

    pub async fn model_factor_analysis_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/models/factor-analysis", body, params).await
    }

    pub async fn portfolio_optimize(&self, body: &Value) -> Result<Value, SecApiError> {
        self.portfolio_optimize_with_params(body, &[]).await
    }

    pub async fn portfolio_optimize_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/portfolio/optimize", body, params).await
    }

    pub async fn portfolio_hedge(&self, body: &Value) -> Result<Value, SecApiError> {
        self.portfolio_hedge_with_params(body, &[]).await
    }

    pub async fn portfolio_hedge_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/portfolio/hedge", body, params).await
    }

    pub async fn portfolio_stress_test(&self, body: &Value) -> Result<Value, SecApiError> {
        self.portfolio_stress_test_with_params(body, &[]).await
    }

    pub async fn portfolio_stress_test_with_params(&self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.post_json_with_params("/v1/portfolio/stress-test", body, params).await
    }

    pub async fn intelligence_security(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/intelligence/security", params).await
    }

    pub async fn intelligence_company(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/intelligence/company", params).await
    }

    pub async fn intelligence_country_report(&self, body: &Value) -> Result<Value, SecApiError> {
        self.post_json("/v1/intelligence/country-report", body).await
    }

    pub async fn intelligence_portfolio(&self, body: &Value) -> Result<Value, SecApiError> {
        self.post_json("/v1/intelligence/portfolio", body).await
    }

    pub async fn intelligence_query(&self, body: &Value) -> Result<Value, SecApiError> {
        self.post_json("/v1/intelligence/query", body).await
    }

    pub async fn intelligence_earnings_preview(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/intelligence/earnings-preview", params).await
    }

    pub async fn intelligence_watchlist(&self, body: &Value) -> Result<Value, SecApiError> {
        self.post_json("/v1/intelligence/watchlist", body).await
    }

    pub async fn intelligence_footnotes_query(&self, body: &Value) -> Result<Value, SecApiError> {
        self.post_json("/v1/intelligence/footnotes/query", body).await
    }

    pub async fn volatility_signal(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/signals/volatility", params).await
    }

    pub async fn mcp_info(&self) -> Result<Value, SecApiError> {
        self.get("/mcp", &[]).await
    }

    pub async fn delete_api_key(&self, key_id: &str) -> Result<(), SecApiError> {
        let url = format!("{}/v1/api_keys/{}", self.base_url.trim_end_matches('/'), key_id);
        let response = self.http.delete(url).headers(self.headers()?).send().await?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let text = response.text().await?;
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Err(SecApiError::Api { status, body })
    }

    pub async fn segmented_revenues(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/statements/segmented-revenues", params).await
    }

    pub async fn segmented_facts(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/statements/segmented-facts", params).await
    }

    pub async fn pension_benefit_schedule(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/filings/pension-benefit-schedule", params).await
    }

    pub async fn share_float(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/statements/share-float", params).await
    }

    pub async fn board_composition(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/board", params).await
    }

    pub async fn nport_holdings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/funds/nport/holdings", params).await
    }

    pub async fn insiders(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/insiders", params).await
    }

    pub async fn beneficial_ownership(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/owners/13d-13g", params).await
    }

    pub async fn compensation(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/compensation", params).await
    }

    pub async fn ma_events(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/events/ma", params).await
    }

    pub async fn voting_results_events(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/events/voting-results", params).await
    }

    // Dilution endpoints (OMNI-3091). All accept ?view=agent except dilution_coverage,
    // whose route returns a small rollup with no agent shape.
    pub async fn dilution_events(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/events", params).await
    }

    pub async fn dilution_event_detail(&self, event_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/dilution/events/{}", urlencoding::encode(event_id)), params).await
    }

    pub async fn dilution_warrants(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/warrants", params).await
    }

    pub async fn dilution_convertibles(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/convertibles", params).await
    }

    pub async fn dilution_rofr(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/rofr", params).await
    }

    pub async fn dilution_lockups(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/lockups", params).await
    }

    pub async fn dilution_cash_position(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/cash-position", params).await
    }

    pub async fn dilution_corporate_actions(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/corporate-actions", params).await
    }

    pub async fn dilution_nasdaq_compliance(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/nasdaq-compliance", params).await
    }

    pub async fn dilution_ratings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/ratings", params).await
    }

    pub async fn dilution_reverse_splits(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/reverse-splits", params).await
    }

    /// Singleton — `params` must include a `("ticker", &str)` entry; route returns 400 without one.
    pub async fn dilution_score(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/score", params).await
    }

    pub async fn dilution_share_float_history(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/share-float-history", params).await
    }

    pub async fn dilution_coverage(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/dilution/coverage", params).await
    }

    pub async fn form_144_filings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/forms/144", params).await
    }

    pub async fn company_subsidiaries(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/companies/subsidiaries", params).await
    }

    pub async fn enforcement_actions(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/events/enforcement", params).await
    }

    pub async fn earnings_transcripts(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/earnings/transcripts", params).await
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<Value, SecApiError> {
        self.post_json_with_params(path, body, &[]).await
    }

    async fn post_json_with_params(&self, path: &str, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let response = self
            .http
            .post(url)
            .headers(self.headers()?)
            .query(params)
            .json(body)
            .send()
            .await?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        if (200..300).contains(&status) {
            return serde_json::from_str(&text).map_err(SecApiError::JsonDecode);
        }
        let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Err(SecApiError::Api { status, body })
    }
}
