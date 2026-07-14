use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER, USER_AGENT};
use serde_json::{json, Map, Value};
use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::time::{Duration, SystemTime};

const DEFAULT_API_VERSION: &str = "2026-03-19";
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_RETRY_MAX_RETRIES: usize = 2;
const DEFAULT_RETRY_INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const DEFAULT_RETRY_MAX_BACKOFF: Duration = Duration::from_secs(2);
const SDK_USER_AGENT: &str = concat!("secapi-rust/", env!("CARGO_PKG_VERSION"));

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

/// `mode=` selector for semantic search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticSearchMode {
    Keyword,
    Semantic,
    Hybrid,
}

impl SemanticSearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}

/// Owned query params produced by typed request builders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryParams {
    entries: Vec<(String, String)>,
}

impl QueryParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let key = key.trim();
        let value = value.into();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return;
        }
        self.entries.retain(|(existing, _)| existing != key);
        self.entries.push((key.to_string(), value.to_string()));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn as_pairs(&self) -> Vec<(&str, &str)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect()
    }

    pub fn into_pairs(self) -> Vec<(String, String)> {
        self.entries
    }
}

fn query_params_from_pairs(params: &[(&str, &str)]) -> QueryParams {
    let mut next = QueryParams::new();
    for (key, value) in params {
        next.set(*key, *value);
    }
    next
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SituationWatchDelivery {
    Email(String),
    OrganizationWebhook,
}

impl SituationWatchDelivery {
    fn into_json(self) -> Result<Value, SecApiError> {
        match self {
            Self::Email(email) => {
                let email = email.trim();
                if email.is_empty() {
                    return Err(client_validation_error("situations.watch email delivery requires a non-empty email"));
                }
                Ok(json!({"type": "email", "config": {"to": email}}))
            }
            Self::OrganizationWebhook => Ok(json!({"type": "webhook", "config": {"organizationEventFanout": true}})),
        }
    }
}

fn client_validation_error(message: impl Into<String>) -> SecApiError {
    SecApiError::Api {
        status: 0,
        body: json!({
            "code": "client_validation_error",
            "message": message.into(),
        }),
    }
}

fn is_situation_id(value: &str) -> bool {
    value.len() == 24
        && value.starts_with("sit_")
        && value[4..].chars().all(|entry| entry.is_ascii_hexdigit() && !entry.is_ascii_uppercase())
}

fn validate_situation_watch_filters(filters: &Value) -> Result<Value, SecApiError> {
    let Some(object) = filters.as_object() else {
        return Err(client_validation_error("situations.watch filters must be an object"));
    };
    if object.is_empty() {
        return Err(client_validation_error("situations.watch requires at least one non-empty list filter"));
    }
    let allowed_keys: HashSet<&str> = ["situationIds", "types", "subtypes", "statuses", "tickers", "sectors"].into_iter().collect();
    let allowed_types: HashSet<&str> = [
        "merger", "tender_offer", "going_private", "spin_off", "divestiture",
        "activist_campaign", "restructuring", "bankruptcy", "liquidation",
        "strategic_review", "capital_return", "capital_raise", "spac", "delisting",
        "relisting", "litigation", "management_change", "domicile_change",
        "demutualization", "other",
    ].into_iter().collect();
    let allowed_subtypes: HashSet<&str> = [
        "definitive", "preliminary", "unsolicited", "rumor_response", "scheme_of_arrangement", "spac_merger",
        "self_tender", "third_party", "exchange_offer", "management_buyout", "sponsor_buyout", "squeeze_out",
        "spin_off", "split_off", "carve_out_ipo", "asset_sale", "joint_venture", "carve_out", "stake_disclosure",
        "proxy_contest", "cooperation_agreement", "settlement", "debt_for_equity_swap", "out_of_court", "operational",
        "chapter_11", "chapter_7", "chapter_15", "administration", "prepackaged", "emergence", "plan_of_liquidation",
        "dissolution", "formal_alternatives", "sale_process", "buyback_authorization", "special_dividend", "recapitalization",
        "rights_offering", "public_offering", "private_placement", "pipe", "atm_program", "ipo", "extension",
        "trust_liquidation", "forced", "voluntary", "uplisting", "otc_relisting", "won", "lost", "settled",
        "ceo", "cfo", "chair", "board", "redomiciliation",
    ].into_iter().collect();
    let allowed_statuses: HashSet<&str> = ["rumored", "announced", "pending", "completed", "terminated", "expired"].into_iter().collect();
    let mut normalized = Map::new();
    for (key, value) in object {
        if !allowed_keys.contains(key.as_str()) {
            return Err(client_validation_error(format!("situations.watch has unsupported filter key: {key}")));
        }
        let Some(array) = value.as_array() else {
            return Err(client_validation_error(format!("situations.watch filter {key} must be a non-empty list")));
        };
        if array.is_empty() {
            return Err(client_validation_error(format!("situations.watch filter {key} must be a non-empty list")));
        }
        let mut values = Vec::with_capacity(array.len());
        for entry in array {
            let Some(raw) = entry.as_str() else {
                return Err(client_validation_error(format!("situations.watch filter {key} values must be strings")));
            };
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(client_validation_error(format!("situations.watch filter {key} cannot contain blank values")));
            }
            values.push(Value::String(trimmed.to_string()));
        }
        normalized.insert(key.clone(), Value::Array(values));
    }
    if let Some(types) = normalized.get("types").and_then(Value::as_array) {
        if types.len() > 50 || types.iter().any(|value| !value.as_str().is_some_and(|entry| allowed_types.contains(entry))) {
            return Err(client_validation_error("situations.watch types must be canonical situation types (maximum 50)"));
        }
    }
    if let Some(subtypes) = normalized.get("subtypes").and_then(Value::as_array) {
        if subtypes.len() > 100 || subtypes.iter().any(|value| !value.as_str().is_some_and(|entry| allowed_subtypes.contains(entry))) {
            return Err(client_validation_error("situations.watch subtypes must be canonical situation subtypes (maximum 100)"));
        }
    }
    if let Some(statuses) = normalized.get("statuses").and_then(Value::as_array) {
        if statuses.len() > 10 || statuses.iter().any(|value| !value.as_str().is_some_and(|entry| allowed_statuses.contains(entry))) {
            return Err(client_validation_error("situations.watch statuses must be canonical lifecycle statuses (maximum 10)"));
        }
    }
    if let Some(situation_ids) = normalized.get("situationIds").and_then(Value::as_array) {
        if situation_ids.len() > 50 || situation_ids.iter().any(|value| !value.as_str().is_some_and(is_situation_id)) {
            return Err(client_validation_error("situations.watch situationIds must be canonical ids (maximum 50)"));
        }
    }
    if normalized.get("tickers").and_then(Value::as_array).map_or(0, Vec::len) > 200
        || normalized.get("sectors").and_then(Value::as_array).map_or(0, Vec::len) > 200
    {
        return Err(client_validation_error("situations.watch tickers and sectors allow at most 200 values"));
    }
    Ok(Value::Object(normalized))
}

fn page_items(page: &Value) -> Result<Vec<Value>, PageIteratorError> {
    for key in ["data", "items", "results", "sections", "filings"] {
        let Some(value) = page.get(key) else {
            continue;
        };
        let Some(items) = value.as_array() else {
            return Err(PageIteratorError::Pagination {
                message: format!("pagination field {key:?} is {}, want array", json_type_name(value)),
            });
        };
        return Ok(items.clone());
    }
    Ok(Vec::new())
}

fn next_cursor(page: &Value) -> Option<String> {
    if page.get("hasMore").and_then(Value::as_bool) == Some(false)
        || page.get("has_more").and_then(Value::as_bool) == Some(false)
    {
        return None;
    }
    json_string_field(page, &["nextCursor", "next_cursor"]).map(str::to_string)
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveEntityRequest {
    params: QueryParams,
}

impl ResolveEntityRequest {
    pub fn new() -> Self {
        Self::default()
    }

    fn param(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.set(key, value);
        self
    }

    pub fn ticker(self, value: impl Into<String>) -> Self { self.param("ticker", value) }
    pub fn cik(self, value: impl Into<String>) -> Self { self.param("cik", value) }
    pub fn figi(self, value: impl Into<String>) -> Self { self.param("figi", value) }
    pub fn composite_figi(self, value: impl Into<String>) -> Self { self.param("composite_figi", value) }
    pub fn share_class_figi(self, value: impl Into<String>) -> Self { self.param("share_class_figi", value) }
    pub fn isin(self, value: impl Into<String>) -> Self { self.param("isin", value) }
    pub fn cusip(self, value: impl Into<String>) -> Self { self.param("cusip", value) }
    pub fn name(self, value: impl Into<String>) -> Self { self.param("name", value) }
    pub fn view(self, value: ResponseView) -> Self { self.param("view", value.as_str()) }
    pub fn extra(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.param(&key, value)
    }
    pub fn params(&self) -> QueryParams { self.params.clone() }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LatestFilingRequest {
    params: QueryParams,
}

impl LatestFilingRequest {
    pub fn new() -> Self {
        Self::default()
    }

    fn param(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.set(key, value);
        self
    }

    pub fn ticker(self, value: impl Into<String>) -> Self { self.param("ticker", value) }
    pub fn cik(self, value: impl Into<String>) -> Self { self.param("cik", value) }
    pub fn form(self, value: impl Into<String>) -> Self { self.param("form", value) }
    pub fn fp(self, value: impl Into<String>) -> Self { self.param("fp", value) }
    pub fn quarter(self, value: impl Into<String>) -> Self { self.param("quarter", value) }
    pub fn filing_year(self, value: impl Into<String>) -> Self { self.param("filing_year", value) }
    pub fn fy(self, value: impl Into<String>) -> Self { self.param("fy", value) }
    pub fn year(self, value: impl Into<String>) -> Self { self.param("year", value) }
    pub fn view(self, value: ResponseView) -> Self { self.param("view", value.as_str()) }
    pub fn extra(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.param(&key, value)
    }
    pub fn params(&self) -> QueryParams { self.params.clone() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestSectionRequest {
    section_key: String,
    params: QueryParams,
}

impl LatestSectionRequest {
    pub fn new(section_key: impl Into<String>) -> Self {
        Self {
            section_key: section_key.into(),
            params: QueryParams::new(),
        }
    }

    fn param(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.set(key, value);
        self
    }

    pub fn section_key(&self) -> &str { &self.section_key }
    pub fn ticker(self, value: impl Into<String>) -> Self { self.param("ticker", value) }
    pub fn cik(self, value: impl Into<String>) -> Self { self.param("cik", value) }
    pub fn form(self, value: impl Into<String>) -> Self { self.param("form", value) }
    pub fn fp(self, value: impl Into<String>) -> Self { self.param("fp", value) }
    pub fn quarter(self, value: impl Into<String>) -> Self { self.param("quarter", value) }
    pub fn filing_year(self, value: impl Into<String>) -> Self { self.param("filing_year", value) }
    pub fn fy(self, value: impl Into<String>) -> Self { self.param("fy", value) }
    pub fn year(self, value: impl Into<String>) -> Self { self.param("year", value) }
    pub fn mode(self, value: impl Into<String>) -> Self { self.param("mode", value) }
    pub fn extra(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.param(&key, value)
    }
    pub fn params(&self) -> QueryParams { self.params.clone() }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SemanticSearchRequest {
    params: QueryParams,
}

impl SemanticSearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self::default().query(query)
    }

    fn param(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params.set(key, value);
        self
    }

    pub fn query(self, value: impl Into<String>) -> Self { self.param("q", value) }
    pub fn ticker(self, value: impl Into<String>) -> Self { self.param("ticker", value) }
    pub fn cik(self, value: impl Into<String>) -> Self { self.param("cik", value) }
    pub fn form(self, value: impl Into<String>) -> Self { self.param("form", value) }
    pub fn filing_year(self, value: impl Into<String>) -> Self { self.param("filing_year", value) }
    pub fn fy(self, value: impl Into<String>) -> Self { self.param("fy", value) }
    pub fn year(self, value: impl Into<String>) -> Self { self.param("year", value) }
    pub fn mode(self, value: SemanticSearchMode) -> Self { self.param("mode", value.as_str()) }
    pub fn limit(self, value: impl Into<String>) -> Self { self.param("limit", value) }
    pub fn view(self, value: ResponseView) -> Self { self.param("view", value.as_str()) }
    pub fn extra(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        self.param(&key, value)
    }
    pub fn params(&self) -> QueryParams { self.params.clone() }
}

/// Retry settings for safe read requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_RETRY_MAX_RETRIES,
            initial_backoff: DEFAULT_RETRY_INITIAL_BACKOFF,
            max_backoff: DEFAULT_RETRY_MAX_BACKOFF,
        }
    }
}

impl RetryConfig {
    pub fn disabled() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }
}

/// Bounds for cursor pagination. Leave fields unset for an unbounded iterator.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaginationOptions {
    pub max_pages: Option<usize>,
    pub max_items: Option<usize>,
}

impl PaginationOptions {
    pub fn max_pages(mut self, value: usize) -> Self {
        self.max_pages = Some(value);
        self
    }

    pub fn max_items(mut self, value: usize) -> Self {
        self.max_items = Some(value);
        self
    }
}

/// Errors returned while consuming a [`PageIterator`].
#[derive(Debug)]
pub enum PageIteratorError {
    SecApi(SecApiError),
    Pagination { message: String },
}

impl fmt::Display for PageIteratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecApi(error) => write!(f, "{error}"),
            Self::Pagination { message } => write!(f, "pagination error: {message}"),
        }
    }
}

impl std::error::Error for PageIteratorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SecApi(error) => Some(error),
            Self::Pagination { .. } => None,
        }
    }
}

impl From<SecApiError> for PageIteratorError {
    fn from(value: SecApiError) -> Self {
        Self::SecApi(value)
    }
}

/// Async pull iterator for SEC API cursor endpoints.
pub struct PageIterator<'a> {
    client: &'a SecApiClient,
    path: String,
    params: QueryParams,
    options: PaginationOptions,
    buffer: VecDeque<Value>,
    pending_error: Option<PageIteratorError>,
    done: bool,
    no_more_pages: bool,
    pages: usize,
    yielded: usize,
    seen_cursors: HashSet<String>,
}

impl<'a> PageIterator<'a> {
    fn new(client: &'a SecApiClient, path: impl Into<String>, params: &[(&str, &str)], options: PaginationOptions) -> Self {
        let params = query_params_from_pairs(params);
        let path = path.into();
        let path = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        let seen_cursors = params
            .entries
            .iter()
            .find_map(|(key, value)| (key == "cursor").then(|| value.trim().to_string()))
            .filter(|value| !value.is_empty())
            .into_iter()
            .collect();
        Self {
            client,
            path,
            params,
            options,
            buffer: VecDeque::new(),
            pending_error: None,
            done: false,
            no_more_pages: false,
            pages: 0,
            yielded: 0,
            seen_cursors,
        }
    }

    pub async fn next(&mut self) -> Result<Option<Value>, PageIteratorError> {
        if self.done {
            return Ok(None);
        }
        loop {
            if let Some(max_items) = self.options.max_items {
                if self.yielded >= max_items {
                    self.done = true;
                    return Ok(None);
                }
            }
            if let Some(item) = self.buffer.pop_front() {
                self.yielded += 1;
                return Ok(Some(item));
            }
            if let Some(error) = self.pending_error.take() {
                self.done = true;
                return Err(error);
            }
            if self.no_more_pages {
                self.done = true;
                return Ok(None);
            }
            if let Some(max_pages) = self.options.max_pages {
                if self.pages >= max_pages {
                    self.done = true;
                    return Ok(None);
                }
            }
            self.fetch_next_page().await?;
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages
    }

    pub fn item_count(&self) -> usize {
        self.yielded
    }

    async fn fetch_next_page(&mut self) -> Result<(), PageIteratorError> {
        let pairs = self.params.as_pairs();
        let page = self.client.get(&self.path, &pairs).await?;
        self.pages += 1;
        let items = page_items(&page)?;
        let page_item_count = items.len();
        self.buffer = items.into();

        let Some(cursor) = next_cursor(&page) else {
            self.no_more_pages = true;
            return Ok(());
        };
        if !self.seen_cursors.insert(cursor.clone()) {
            self.pending_error = Some(PageIteratorError::Pagination {
                message: format!("SEC API pagination cursor repeated: {cursor}"),
            });
            return Ok(());
        }
        if page_item_count == 0 {
            self.no_more_pages = true;
            return Ok(());
        }
        self.params.set("cursor", cursor);
        Ok(())
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
                write!(f, "Auth credential contains characters that cannot be sent in a request header")
            }
            Self::JsonDecode(e) => write!(f, "invalid JSON response: {e}"),
            Self::Api { status, body } => {
                write!(f, "API error HTTP {status}")?;
                if let Some(code) = self.code() {
                    write!(f, " code={code}")?;
                }
                if let Some(request_id) = self.request_id() {
                    write!(f, " request_id={request_id}")?;
                }
                if let Some(message) = self.message() {
                    write!(f, ": {message}")?;
                }
                write!(f, " body={body}")
            }
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

impl SecApiError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub fn body(&self) -> Option<&Value> {
        match self {
            Self::Api { body, .. } => Some(body),
            _ => None,
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Api { body, .. } => json_string_field(body, &["requestId", "request_id"]),
            _ => None,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Api { body, .. } => {
                json_string_field(body, &["code", "errorCode", "error_code"])
                    .or_else(|| nested_error_string_field(body, &["code", "errorCode", "error_code", "type"]))
            }
            _ => None,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Api { body, .. } => {
                json_string_field(body, &["message", "detail", "title"])
                    .or_else(|| nested_error_string_field(body, &["message", "detail", "title"]))
                    .or_else(|| body.get("error").and_then(Value::as_str))
            }
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

fn mcp_tool_call_body(tool_name: &str, arguments: &[(&str, Value)], id: Option<&str>) -> Value {
    let argument_map: serde_json::Map<String, Value> = arguments
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect();
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or("secapi-rust"),
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": argument_map,
        },
    })
}

fn api_error(status: u16, text: String, header_request_id: Option<String>) -> SecApiError {
    let mut body = serde_json::from_str(&text).unwrap_or(Value::String(text));
    if let Some(request_id) = header_request_id {
        match &mut body {
            Value::Object(map) => {
                let has_body_request_id = ["requestId", "request_id"].iter().any(|key| {
                    map.get(*key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .is_some_and(|value| !value.is_empty())
                });
                if !has_body_request_id {
                    map.insert("requestId".to_string(), Value::String(request_id));
                }
            }
            _ => {
                body = json!({
                    "requestId": request_id,
                    "body": body,
                });
            }
        }
    }
    SecApiError::Api { status, body }
}

fn response_request_id(headers: &HeaderMap) -> Option<String> {
    ["request-id", "x-request-id"].iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn json_string_field<'a>(body: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        body.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn nested_error_string_field<'a>(body: &'a Value, keys: &[&str]) -> Option<&'a str> {
    body.get("error").and_then(|error| json_string_field(error, keys))
}

fn default_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(DEFAULT_HTTP_TIMEOUT)
        .build()
        .expect("build default SEC API HTTP client")
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 502 | 503 | 504)
}

fn retry_after_delay(value: Option<&HeaderValue>) -> Option<Duration> {
    let value = value?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let when = httpdate::parse_http_date(value).ok()?;
    Some(when.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO))
}

fn clamp_delay(delay: Duration, max_backoff: Duration) -> Duration {
    if delay > max_backoff {
        max_backoff
    } else {
        delay
    }
}

fn retry_delay(attempt: usize, config: RetryConfig, retry_after: Option<&HeaderValue>) -> Duration {
    if let Some(delay) = retry_after_delay(retry_after) {
        return clamp_delay(delay, config.max_backoff);
    }
    let multiplier = 1_u32.checked_shl(attempt as u32).unwrap_or(u32::MAX);
    clamp_delay(config.initial_backoff.saturating_mul(multiplier), config.max_backoff)
}

pub struct SecApiClient {
    api_key: Option<String>,
    bearer_token: Option<String>,
    base_url: String,
    api_version: String,
    http: reqwest::Client,
    retry_config: RetryConfig,
}

impl SecApiClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key: api_key
                .filter(|value| !value.trim().is_empty())
                .or_else(|| runtime_env(&["SECAPI_API_KEY", "OMNI_DATASTREAM_API_KEY"])),
            bearer_token: runtime_env(&["SECAPI_BEARER_TOKEN", "OMNI_DATASTREAM_BEARER_TOKEN"]),
            base_url: std::env::var("SECAPI_BASE_URL")
                .or_else(|_| std::env::var("SECAPI_API_BASE_URL"))
                .or_else(|_| std::env::var("OMNI_DATASTREAM_BASE_URL"))
                .or_else(|_| std::env::var("OMNI_DATASTREAM_API_BASE_URL"))
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "https://api.secapi.ai".to_string()),
            api_version: DEFAULT_API_VERSION.to_string(),
            http: default_http_client(),
            retry_config: RetryConfig::default(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_bearer_token(mut self, bearer_token: impl Into<String>) -> Self {
        self.bearer_token = Some(bearer_token.into())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.http = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("build SEC API HTTP client with timeout");
        self
    }

    pub fn with_http_client(mut self, http: reqwest::Client) -> Self {
        self.http = http;
        self
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    pub fn without_retries(self) -> Self {
        self.with_retry_config(RetryConfig::disabled())
    }

    pub fn entities(&self) -> EntityService<'_> {
        EntityService { client: self }
    }

    pub fn filings(&self) -> FilingService<'_> {
        FilingService { client: self }
    }

    pub fn sections(&self) -> SectionService<'_> {
        SectionService { client: self }
    }

    pub fn search(&self) -> SearchService<'_> {
        SearchService { client: self }
    }

    pub fn factors(&self) -> FactorService<'_> {
        FactorService { client: self }
    }

    pub fn situations(&self) -> SituationService<'_> {
        SituationService { client: self }
    }

    fn headers(&self) -> Result<HeaderMap, SecApiError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(SDK_USER_AGENT));
        let version_header = HeaderValue::from_str(&self.api_version).unwrap_or_else(|_| {
            HeaderValue::from_static(DEFAULT_API_VERSION)
        });
        headers.insert("secapi-version", version_header);
        if let Some(bearer_token) = &self.bearer_token {
            let value = HeaderValue::from_str(&format!("Bearer {bearer_token}"))
                .map_err(|_| SecApiError::InvalidApiKeyHeader)?;
            headers.insert(AUTHORIZATION, value);
        }
        if let Some(api_key) = &self.api_key {
            let value = HeaderValue::from_str(api_key).map_err(|_| SecApiError::InvalidApiKeyHeader)?;
            headers.insert("x-api-key", value);
        }
        Ok(headers)
    }

    async fn get(&self, path: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        for attempt in 0..=self.retry_config.max_retries {
            let response = self
                .http
                .get(&url)
                .headers(self.headers()?)
                .query(params)
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if is_retryable_status(status) && attempt < self.retry_config.max_retries {
                        let retry_after = response.headers().get(RETRY_AFTER).cloned();
                        drop(response);
                        tokio::time::sleep(retry_delay(attempt, self.retry_config, retry_after.as_ref())).await;
                        continue;
                    }
                    let request_id = response_request_id(response.headers());
                    let text = response.text().await?;
                    if (200..300).contains(&status) {
                        return serde_json::from_str(&text).map_err(SecApiError::JsonDecode);
                    }
                    return Err(api_error(status, text, request_id));
                }
                Err(error) => {
                    if attempt < self.retry_config.max_retries {
                        tokio::time::sleep(retry_delay(attempt, self.retry_config, None)).await;
                        continue;
                    }
                    return Err(error.into());
                }
            }
        }
        unreachable!("retry loop always returns")
    }

    async fn get_text(&self, path: &str, params: &[(&str, &str)]) -> Result<String, SecApiError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        for attempt in 0..=self.retry_config.max_retries {
            let response = self
                .http
                .get(&url)
                .headers(self.headers()?)
                .query(params)
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if is_retryable_status(status) && attempt < self.retry_config.max_retries {
                        let retry_after = response.headers().get(RETRY_AFTER).cloned();
                        drop(response);
                        tokio::time::sleep(retry_delay(attempt, self.retry_config, retry_after.as_ref())).await;
                        continue;
                    }
                    let request_id = response_request_id(response.headers());
                    let text = response.text().await?;
                    if (200..300).contains(&status) {
                        return Ok(text);
                    }
                    return Err(api_error(status, text, request_id));
                }
                Err(error) => {
                    if attempt < self.retry_config.max_retries {
                        tokio::time::sleep(retry_delay(attempt, self.retry_config, None)).await;
                        continue;
                    }
                    return Err(error.into());
                }
            }
        }
        unreachable!("retry loop always returns")
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

    pub async fn resolve_entity_with(&self, request: &ResolveEntityRequest) -> Result<Value, SecApiError> {
        let params = request.params();
        let pairs = params.as_pairs();
        self.resolve_entity(&pairs).await
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

    pub fn paginate(&self, path: impl Into<String>, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'_> {
        PageIterator::new(self, path, params, options)
    }

    pub fn paginate_entities(&self, params: &[(&str, &str)]) -> PageIterator<'_> {
        self.paginate_entities_with_options(params, PaginationOptions::default())
    }

    pub fn paginate_entities_with_options(&self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'_> {
        self.paginate("/v1/entities", params, options)
    }

    pub fn paginate_filings(&self, params: &[(&str, &str)]) -> PageIterator<'_> {
        self.paginate_filings_with_options(params, PaginationOptions::default())
    }

    pub fn paginate_filings_with_options(&self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'_> {
        self.paginate("/v1/filings", params, options)
    }

    pub fn paginate_sections(&self, params: &[(&str, &str)]) -> PageIterator<'_> {
        self.paginate_sections_with_options(params, PaginationOptions::default())
    }

    pub fn paginate_sections_with_options(&self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'_> {
        self.paginate("/v1/sections/search", params, options)
    }

    pub async fn search_fulltext(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/search/fulltext", params).await
    }

    pub async fn semantic_search(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/search/semantic", params).await
    }

    pub async fn semantic_search_with(&self, request: &SemanticSearchRequest) -> Result<Value, SecApiError> {
        let params = request.params();
        let pairs = params.as_pairs();
        self.semantic_search(&pairs).await
    }

    pub async fn latest_filing(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/filings/latest", params).await
    }

    pub async fn latest_filing_with(&self, request: &LatestFilingRequest) -> Result<Value, SecApiError> {
        let params = request.params();
        let pairs = params.as_pairs();
        self.latest_filing(&pairs).await
    }

    pub async fn latest_section(&self, section_key: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/filings/latest/sections/{}", urlencoding::encode(section_key)), params).await
    }

    pub async fn latest_section_with(&self, request: &LatestSectionRequest) -> Result<Value, SecApiError> {
        let params = request.params();
        let pairs = params.as_pairs();
        self.latest_section(request.section_key(), &pairs).await
    }

    pub async fn filing_by_accession(&self, accession_number: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/filings/{}", urlencoding::encode(accession_number)), params).await
    }

    pub async fn filing_section_by_accession(
        &self,
        accession_number: &str,
        section_key: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, SecApiError> {
        self.get(
            &format!(
                "/v1/filings/{}/sections/{}",
                urlencoding::encode(accession_number),
                urlencoding::encode(section_key),
            ),
            params,
        ).await
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

    pub async fn list_situations(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations", params).await
    }

    pub async fn get_situation(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/{}", urlencoding::encode(situation_id)), params).await
    }

    pub async fn situations_by_form(&self, form: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/by-form/{}", urlencoding::encode(form)), params).await
    }

    pub async fn situations_feed(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations/feed", params).await
    }

    pub async fn situations_feed_rss(&self, params: &[(&str, &str)]) -> Result<String, SecApiError> {
        self.get_text("/v1/situations/feed.rss", params).await
    }

    pub async fn situations_issues(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations/issues", params).await
    }

    pub async fn situation_issue(&self, issue: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/issues/{}", urlencoding::encode(issue)), params).await
    }

    pub async fn situations_calendar(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations/calendar", params).await
    }

    pub async fn situations_stats(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations/stats", params).await
    }

    pub async fn situations_performance(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/situations/performance", params).await
    }

    pub async fn situation_filings(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/{}/filings", urlencoding::encode(situation_id)), params).await
    }

    pub async fn situation_summary(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/{}/summary", urlencoding::encode(situation_id)), params).await
    }

    pub async fn export_situation(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<String, SecApiError> {
        self.get_text(&format!("/v1/situations/{}/export", urlencoding::encode(situation_id)), params).await
    }

    pub async fn underwrite_situation(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/situations/{}/underwriting-pack", urlencoding::encode(situation_id)), params).await
    }

    pub async fn watch_situations(
        &self,
        filters: &Value,
        delivery: SituationWatchDelivery,
        name: Option<&str>,
        start_at: Option<&str>,
    ) -> Result<Value, SecApiError> {
        let normalized_filters = validate_situation_watch_filters(filters)?;
        let watch_name = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Special Situations watch");
        let mut body = json!({
            "name": watch_name,
            "query": "situations.watch",
            "searchMode": "situation",
            "filters": normalized_filters,
            "delivery": delivery.into_json()?,
        });
        if let Some(start_at) = start_at.map(str::trim).filter(|value| !value.is_empty()) {
            body["startAt"] = Value::String(start_at.to_string());
        }
        self.post_json("/v1/monitors", &body).await
    }

    pub async fn embed_situations(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/embed/situations", params).await
    }

    pub async fn embed_situation(&self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/embed/situations/{}", urlencoding::encode(situation_id)), params).await
    }

    pub async fn embed_situation_issues(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/embed/situations/issues", params).await
    }

    pub async fn embed_situation_issue(&self, issue: &str) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/embed/situations/issues/{}", urlencoding::encode(issue)), &[]).await
    }

    pub async fn macro_search(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/search", params).await
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

    pub async fn macro_credit_ratings(&self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.get("/v1/macro/credit-ratings", params).await
    }

    pub async fn macro_credit_rating(&self, country: &str) -> Result<Value, SecApiError> {
        self.get(&format!("/v1/macro/credit-ratings/{}", urlencoding::encode(country)), &[]).await
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
        self.get(&format!("/v1/stocks/{}/loadings", urlencoding::encode(ticker)), params).await
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
        self.get(&format!("/v1/model-portfolios/{}/factor-view", urlencoding::encode(portfolio_id)), params).await
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

    pub async fn call_mcp_tool(
        &self,
        tool_name: &str,
        arguments: &[(&str, Value)],
        id: Option<&str>,
    ) -> Result<Value, SecApiError> {
        self.post_json("/mcp", &mcp_tool_call_body(tool_name, arguments, id)).await
    }

    pub async fn delete_api_key(&self, key_id: &str) -> Result<(), SecApiError> {
        let url = format!(
            "{}/v1/api_keys/{}",
            self.base_url.trim_end_matches('/'),
            urlencoding::encode(key_id)
        );
        let response = self.http.delete(url).headers(self.headers()?).send().await?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let request_id = response_request_id(response.headers());
        let text = response.text().await?;
        Err(api_error(status, text, request_id))
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
        for attempt in 0..=self.retry_config.max_retries {
            let response = self
                .http
                .post(&url)
                .headers(self.headers()?)
                .query(params)
                .json(body)
                .send()
                .await?;
            let status = response.status().as_u16();
            if status == 429 && attempt < self.retry_config.max_retries {
                let retry_after = response.headers().get(RETRY_AFTER).cloned();
                drop(response);
                tokio::time::sleep(retry_delay(attempt, self.retry_config, retry_after.as_ref())).await;
                continue;
            }
            let request_id = response_request_id(response.headers());
            let text = response.text().await?;
            if (200..300).contains(&status) {
                return serde_json::from_str(&text).map_err(SecApiError::JsonDecode);
            }
            return Err(api_error(status, text, request_id));
        }
        unreachable!("retry loop always returns")
    }
}

#[derive(Clone, Copy)]
pub struct EntityService<'a> {
    client: &'a SecApiClient,
}

impl<'a> EntityService<'a> {
    pub async fn resolve(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.resolve_entity(params).await
    }

    pub async fn resolve_with(self, request: &ResolveEntityRequest) -> Result<Value, SecApiError> {
        self.client.resolve_entity_with(request).await
    }

    pub async fn search(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.search_entities(params).await
    }

    pub fn paginate(self, params: &[(&str, &str)]) -> PageIterator<'a> {
        self.client.paginate_entities(params)
    }

    pub fn paginate_with_options(self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'a> {
        self.client.paginate_entities_with_options(params, options)
    }
}

#[derive(Clone, Copy)]
pub struct FilingService<'a> {
    client: &'a SecApiClient,
}

impl<'a> FilingService<'a> {
    pub async fn search(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.search_filings(params).await
    }

    pub fn paginate(self, params: &[(&str, &str)]) -> PageIterator<'a> {
        self.client.paginate_filings(params)
    }

    pub fn paginate_with_options(self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'a> {
        self.client.paginate_filings_with_options(params, options)
    }

    pub async fn latest(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.latest_filing(params).await
    }

    pub async fn latest_with(self, request: &LatestFilingRequest) -> Result<Value, SecApiError> {
        self.client.latest_filing_with(request).await
    }

    pub async fn by_accession(self, accession_number: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.filing_by_accession(accession_number, params).await
    }
}

#[derive(Clone, Copy)]
pub struct SectionService<'a> {
    client: &'a SecApiClient,
}

impl<'a> SectionService<'a> {
    pub async fn search(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.search_sections(params).await
    }

    pub fn paginate(self, params: &[(&str, &str)]) -> PageIterator<'a> {
        self.client.paginate_sections(params)
    }

    pub fn paginate_with_options(self, params: &[(&str, &str)], options: PaginationOptions) -> PageIterator<'a> {
        self.client.paginate_sections_with_options(params, options)
    }

    pub async fn latest(self, section_key: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.latest_section(section_key, params).await
    }

    pub async fn latest_with(self, request: &LatestSectionRequest) -> Result<Value, SecApiError> {
        self.client.latest_section_with(request).await
    }

    pub async fn by_accession(
        self,
        accession_number: &str,
        section_key: &str,
        params: &[(&str, &str)],
    ) -> Result<Value, SecApiError> {
        self.client.filing_section_by_accession(accession_number, section_key, params).await
    }
}

#[derive(Clone, Copy)]
pub struct SearchService<'a> {
    client: &'a SecApiClient,
}

impl<'a> SearchService<'a> {
    pub async fn fulltext(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.search_fulltext(params).await
    }

    pub async fn semantic(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.semantic_search(params).await
    }

    pub async fn semantic_with(self, request: &SemanticSearchRequest) -> Result<Value, SecApiError> {
        self.client.semantic_search_with(request).await
    }
}

#[derive(Clone, Copy)]
pub struct SituationService<'a> {
    client: &'a SecApiClient,
}

impl<'a> SituationService<'a> {
    pub async fn list(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.list_situations(params).await
    }

    pub async fn get(self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.get_situation(situation_id, params).await
    }

    pub async fn by_form(self, form: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_by_form(form, params).await
    }

    pub async fn feed(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_feed(params).await
    }

    pub async fn feed_rss(self, params: &[(&str, &str)]) -> Result<String, SecApiError> {
        self.client.situations_feed_rss(params).await
    }

    pub async fn issues(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_issues(params).await
    }

    pub async fn issue(self, issue: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situation_issue(issue, params).await
    }

    pub async fn calendar(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_calendar(params).await
    }

    pub async fn stats(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_stats(params).await
    }

    pub async fn performance(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situations_performance(params).await
    }

    pub async fn filings(self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situation_filings(situation_id, params).await
    }

    pub async fn summary(self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.situation_summary(situation_id, params).await
    }

    pub async fn export(self, situation_id: &str, params: &[(&str, &str)]) -> Result<String, SecApiError> {
        self.client.export_situation(situation_id, params).await
    }

    pub async fn underwrite(self, situation_id: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.underwrite_situation(situation_id, params).await
    }

    pub async fn watch(
        self,
        filters: &Value,
        delivery: SituationWatchDelivery,
        name: Option<&str>,
        start_at: Option<&str>,
    ) -> Result<Value, SecApiError> {
        self.client.watch_situations(filters, delivery, name, start_at).await
    }
}

#[derive(Clone, Copy)]
pub struct FactorService<'a> {
    client: &'a SecApiClient,
}

impl<'a> FactorService<'a> {
    pub async fn catalog(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_catalog(params).await
    }

    pub async fn returns(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_returns(params).await
    }

    pub async fn history(self, factor_key: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_history(factor_key, params).await
    }

    pub async fn sparklines(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_sparklines(params).await
    }

    pub async fn returns_intraday(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_returns_intraday(params).await
    }

    pub async fn dashboard(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_dashboard(params).await
    }

    pub async fn regime_performance(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_regime_performance(params).await
    }

    pub async fn correlations(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_correlations(params).await
    }

    pub async fn extreme_moves(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_extreme_moves(params).await
    }

    pub async fn extreme_pairs(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_extreme_pairs(params).await
    }

    pub async fn valuations(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_valuations(params).await
    }

    pub async fn valuation_stocks(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_valuation_stocks(params).await
    }

    pub async fn exposures(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_exposures(params).await
    }

    pub async fn decomposition(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_decomposition(params).await
    }

    pub async fn related_stocks(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_related_stocks(params).await
    }

    pub async fn similarity_pack(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_similarity_pack(params).await
    }

    pub async fn pairs(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_pairs(params).await
    }

    pub async fn pair_history(self, f1: &str, f2: &str, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_pair_history(f1, f2, params).await
    }

    pub async fn bulk_download(self, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_bulk_download(params).await
    }

    pub async fn custom(self, body: &Value) -> Result<Value, SecApiError> {
        self.client.factor_custom(body).await
    }

    pub async fn custom_with_params(self, body: &Value, params: &[(&str, &str)]) -> Result<Value, SecApiError> {
        self.client.factor_custom_with_params(body, params).await
    }
}

fn runtime_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        mpsc::{self, Receiver},
        Mutex,
    };
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn json_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn capture_server(expected_requests: usize) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind capture server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .expect("request target")
                    .to_string();
                tx.send(target).expect("send request target");

                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: application/json\r\n",
                    "content-length: 11\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "{\"ok\":true}"
                );
                stream.write_all(response.as_bytes()).expect("write response");
            }
        });

        (base_url, rx, handle)
    }

    fn response_server(response: String) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind response server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0; 4096];
            stream.read(&mut buffer).expect("read request");
            stream.write_all(response.as_bytes()).expect("write response");
        });

        (base_url, handle)
    }

    fn response_sequence_server(responses: Vec<String>) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind response sequence server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .expect("request target")
                    .to_string();
                tx.send(target).expect("send request target");
                stream.write_all(response.as_bytes()).expect("write response");
            }
        });

        (base_url, rx, handle)
    }

    fn raw_response_sequence_server(responses: Vec<String>) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind raw response sequence server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                tx.send(String::from_utf8_lossy(&buffer[..read]).to_string())
                    .expect("send raw request");
                stream.write_all(response.as_bytes()).expect("write response");
            }
        });

        (base_url, rx, handle)
    }

    fn dropped_connection_then_ok_server() -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind dropped connection server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept request");
                let mut buffer = [0; 4096];
                let read = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .expect("request target")
                    .to_string();
                tx.send(target).expect("send request target");
                if attempt == 0 {
                    continue;
                }
                let response = concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "content-type: application/json\r\n",
                    "content-length: 11\r\n",
                    "connection: close\r\n",
                    "\r\n",
                    "{\"ok\":true}"
                );
                stream.write_all(response.as_bytes()).expect("write response");
            }
        });

        (base_url, rx, handle)
    }

    fn raw_capture_server(response: &'static str) -> (String, Receiver<String>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind raw capture server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).expect("read request");
            tx.send(String::from_utf8_lossy(&buffer[..read]).to_string())
                .expect("send raw request");
            stream.write_all(response.as_bytes()).expect("write response");
        });

        (base_url, rx, handle)
    }

    fn slow_server(delay: Duration) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow server");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set slow server read timeout");
            let mut buffer = [0; 4096];
            stream.read(&mut buffer).expect("read request");
            thread::sleep(delay);
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "content-type: application/json\r\n",
                "content-length: 11\r\n",
                "connection: close\r\n",
                "\r\n",
                "{\"ok\":true}"
            );
            let _ = stream.write_all(response.as_bytes());
        });

        (base_url, handle)
    }

    #[test]
    fn new_client_loads_environment_fallbacks() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_api_key = std::env::var("SECAPI_API_KEY").ok();
        let previous_base_url = std::env::var("SECAPI_BASE_URL").ok();
        std::env::set_var("SECAPI_API_KEY", "env_fallback_api_key");
        std::env::set_var("SECAPI_BASE_URL", "https://env.secapi.test/");

        let client = SecApiClient::new(None);

        assert_eq!(client.api_key.as_deref(), Some("env_fallback_api_key"));
        assert_eq!(client.base_url, "https://env.secapi.test/");

        restore_env("SECAPI_API_KEY", previous_api_key);
        restore_env("SECAPI_BASE_URL", previous_base_url);
    }

    #[test]
    fn new_client_explicit_api_key_overrides_environment_fallback() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_api_key = std::env::var("SECAPI_API_KEY").ok();
        std::env::set_var("SECAPI_API_KEY", "env_fallback_api_key");

        let client = SecApiClient::new(Some("explicit_api_key".to_string()));

        assert_eq!(client.api_key.as_deref(), Some("explicit_api_key"));

        restore_env("SECAPI_API_KEY", previous_api_key);
    }

    #[test]
    fn new_client_loads_bearer_token_environment_fallback() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_secapi_api_key = std::env::var("SECAPI_API_KEY").ok();
        let previous_omni_api_key = std::env::var("OMNI_DATASTREAM_API_KEY").ok();
        let previous_secapi_bearer = std::env::var("SECAPI_BEARER_TOKEN").ok();
        let previous_omni_bearer = std::env::var("OMNI_DATASTREAM_BEARER_TOKEN").ok();
        std::env::remove_var("SECAPI_API_KEY");
        std::env::remove_var("OMNI_DATASTREAM_API_KEY");
        std::env::remove_var("SECAPI_BEARER_TOKEN");
        std::env::set_var("OMNI_DATASTREAM_BEARER_TOKEN", "bearer_OMNI_FALLBACK");

        let client = SecApiClient::new(None);

        assert_eq!(client.api_key.as_deref(), None);
        assert_eq!(client.bearer_token.as_deref(), Some("bearer_OMNI_FALLBACK"));

        restore_env("SECAPI_API_KEY", previous_secapi_api_key);
        restore_env("OMNI_DATASTREAM_API_KEY", previous_omni_api_key);
        restore_env("SECAPI_BEARER_TOKEN", previous_secapi_bearer);
        restore_env("OMNI_DATASTREAM_BEARER_TOKEN", previous_omni_bearer);
    }

    #[test]
    fn new_client_loads_compatibility_environment_fallbacks() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_secapi_api_key = std::env::var("SECAPI_API_KEY").ok();
        let previous_secapi_base_url = std::env::var("SECAPI_BASE_URL").ok();
        let previous_api_key = std::env::var("OMNI_DATASTREAM_API_KEY").ok();
        let previous_base_url = std::env::var("OMNI_DATASTREAM_BASE_URL").ok();
        let previous_api_base_url = std::env::var("OMNI_DATASTREAM_API_BASE_URL").ok();
        std::env::remove_var("SECAPI_API_KEY");
        std::env::remove_var("SECAPI_BASE_URL");
        std::env::set_var("OMNI_DATASTREAM_API_KEY", "omni_fallback_api_key");
        std::env::remove_var("OMNI_DATASTREAM_BASE_URL");
        std::env::set_var("OMNI_DATASTREAM_API_BASE_URL", "https://omni-api.secapi.test/");

        let client = SecApiClient::new(None);

        assert_eq!(client.api_key.as_deref(), Some("omni_fallback_api_key"));
        assert_eq!(client.base_url, "https://omni-api.secapi.test/");

        restore_env("SECAPI_API_KEY", previous_secapi_api_key);
        restore_env("SECAPI_BASE_URL", previous_secapi_base_url);
        restore_env("OMNI_DATASTREAM_API_KEY", previous_api_key);
        restore_env("OMNI_DATASTREAM_BASE_URL", previous_base_url);
        restore_env("OMNI_DATASTREAM_API_BASE_URL", previous_api_base_url);
    }

    #[tokio::test]
    async fn with_http_client_uses_injected_client_configuration() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        );
        let (base_url, rx, handle) = raw_capture_server(response);
        let mut default_headers = HeaderMap::new();
        default_headers.insert("x-sdk-test", HeaderValue::from_static("custom-client"));
        let http = reqwest::Client::builder()
            .default_headers(default_headers)
            .timeout(Duration::from_secs(5))
            .build()
            .expect("build injected HTTP client");
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_http_client(http);

        client.health().await.unwrap();

        let raw_request = rx.recv_timeout(Duration::from_secs(2)).expect("raw request");
        handle.join().expect("raw capture server thread");

        assert!(raw_request.contains("x-sdk-test: custom-client"));
        assert!(raw_request.contains("accept: application/json"));
        assert!(raw_request.contains(&format!(
            "user-agent: secapi-rust/{}",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(raw_request.contains("content-type: application/json"));
        assert!(raw_request.contains("secapi-version: 2026-03-19"));
    }

    #[tokio::test]
    async fn with_bearer_token_sends_authorization_header() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        );
        let (base_url, rx, handle) = raw_capture_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_bearer_token("bearer_explicit_token");

        client.health().await.unwrap();

        let raw_request = rx.recv_timeout(Duration::from_secs(2)).expect("raw request");
        handle.join().expect("raw capture server thread");

        assert!(raw_request.contains("authorization: Bearer bearer_explicit_token"));
    }

    #[tokio::test]
    async fn with_timeout_bounds_slow_responses() {
        let (base_url, handle) = slow_server(Duration::from_millis(250));
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_timeout(Duration::from_millis(50))
            .without_retries();

        let error = client.health().await.expect_err("expected timeout error");
        handle.join().expect("slow server thread");

        match error {
            SecApiError::Request(error) => assert!(error.is_timeout(), "expected timeout, got {error}"),
            other => panic!("expected request timeout, got {other:?}"),
        }
    }

    #[test]
    fn retry_config_disabled_turns_off_retries() {
        assert_eq!(RetryConfig::disabled().max_retries, 0);
        assert_eq!(RetryConfig::disabled().initial_backoff, Duration::ZERO);
        assert_eq!(RetryConfig::disabled().max_backoff, Duration::ZERO);
    }

    #[test]
    fn retry_after_delay_clamps_to_max_backoff() {
        let config = RetryConfig {
            max_retries: 2,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(250),
        };
        let header = HeaderValue::from_static("5");

        assert_eq!(retry_delay(0, config, Some(&header)), Duration::from_millis(250));
    }

    #[test]
    fn retry_after_delay_clamps_to_zero_max_backoff() {
        let config = RetryConfig {
            max_retries: 1,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::ZERO,
        };
        let header = HeaderValue::from_static("5");

        assert_eq!(retry_delay(0, config, Some(&header)), Duration::ZERO);
    }

    #[tokio::test]
    async fn safe_get_retries_retryable_status_then_succeeds() {
        let unavailable = concat!(
            "HTTP/1.1 503 Service Unavailable\r\n",
            "content-type: application/json\r\n",
            "content-length: 31\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"error\":\"temporarily down\"}"
        )
        .to_string();
        let ok = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        )
        .to_string();
        let (base_url, rx, handle) = response_sequence_server(vec![unavailable, ok]);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_retry_config(RetryConfig {
                max_retries: 2,
                initial_backoff: Duration::ZERO,
                max_backoff: Duration::ZERO,
            });

        let value = client.health().await.unwrap();
        let targets: Vec<String> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("response sequence server thread");

        assert_eq!(value["ok"], true);
        assert_eq!(targets, vec!["/healthz", "/healthz"]);
    }

    #[tokio::test]
    async fn safe_get_retries_transport_failure_then_succeeds() {
        let (base_url, rx, handle) = dropped_connection_then_ok_server();
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_retry_config(RetryConfig {
                max_retries: 2,
                initial_backoff: Duration::ZERO,
                max_backoff: Duration::ZERO,
            });

        let value = client.health().await.unwrap();
        let targets: Vec<String> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("dropped connection server thread");

        assert_eq!(value["ok"], true);
        assert_eq!(targets, vec!["/healthz", "/healthz"]);
    }

    #[tokio::test]
    async fn mutating_post_does_not_retry_transient_server_status() {
        let body = r#"{"error":"try later"}"#;
        let response = format!(
            "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_retry_config(RetryConfig {
                max_retries: 2,
                initial_backoff: Duration::ZERO,
                max_backoff: Duration::ZERO,
            });

        let error = client
            .factor_custom(&json!({"prompt": "quality minus junk"}))
            .await
            .expect_err("expected API error");
        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(error.status(), Some(503));
        assert_eq!(target, "/v1/factors/custom");
    }

    #[tokio::test]
    async fn mutating_post_retries_rate_limit_status_then_succeeds() {
        let limited = concat!(
            "HTTP/1.1 429 Too Many Requests\r\n",
            "content-type: application/json\r\n",
            "retry-after: 0\r\n",
            "content-length: 22\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"error\":\"rate limit\"}"
        )
        .to_string();
        let ok = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        )
        .to_string();
        let (base_url, rx, handle) = raw_response_sequence_server(vec![limited, ok]);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .with_retry_config(RetryConfig {
                max_retries: 2,
                initial_backoff: Duration::ZERO,
                max_backoff: Duration::ZERO,
            });
        let body = json!({"prompt": "quality minus junk"});

        let value = client
            .factor_custom_with_params(&body, &[("view", "agent")])
            .await
            .unwrap();
        let requests: Vec<String> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("raw request"))
            .collect();
        handle.join().expect("response sequence server thread");

        assert_eq!(value["ok"], true);
        for request in requests {
            assert!(request.starts_with("POST /v1/factors/custom?view=agent HTTP/1.1"));
            assert!(request.contains(r#"{"prompt":"quality minus junk"}"#));
        }
    }

    fn restore_env(name: &str, previous: Option<String>) {
        if let Some(value) = previous {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }

    #[tokio::test]
    async fn api_error_exposes_json_metadata_and_request_id() {
        let body = r#"{"code":"invalid_request","message":"ticker is required","requestId":"req_json_123"}"#;
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.latest_filing(&[]).await.expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(400));
        assert_eq!(error.request_id(), Some("req_json_123"));
        assert_eq!(error.code(), Some("invalid_request"));
        assert_eq!(error.message(), Some("ticker is required"));
        assert_eq!(error.body().and_then(|body| body.get("code")), Some(&json!("invalid_request")));
        assert!(error.to_string().contains("request_id=req_json_123"));
    }

    #[tokio::test]
    async fn api_error_preserves_header_request_id_when_body_omits_it() {
        let body = r#"{"code":"invalid_request","message":"ticker is required"}"#;
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\nx-request-id: req_header_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.latest_filing(&[]).await.expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(400));
        assert_eq!(error.request_id(), Some("req_header_123"));
        assert_eq!(error.code(), Some("invalid_request"));
        assert_eq!(error.message(), Some("ticker is required"));
        assert_eq!(error.body().and_then(|body| body.get("requestId")), Some(&json!("req_header_123")));
        assert!(error.to_string().contains("request_id=req_header_123"));
    }

    #[tokio::test]
    async fn api_error_preserves_header_request_id_for_plain_text_body() {
        let body = "upstream unavailable";
        let response = format!(
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/plain\r\nrequest-id: req_plain_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.health().await.expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(502));
        assert_eq!(error.request_id(), Some("req_plain_123"));
        assert_eq!(error.body().and_then(|body| body.get("body")), Some(&json!("upstream unavailable")));
        assert!(error.to_string().contains("request_id=req_plain_123"));
    }

    #[tokio::test]
    async fn api_error_preserves_header_request_id_for_post_errors() {
        let body = r#"{"error":"try later"}"#;
        let response = format!(
            "HTTP/1.1 503 Service Unavailable\r\ncontent-type: application/json\r\nx-request-id: req_post_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, _rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.factor_custom(&json!({"prompt": "quality minus junk"})).await.expect_err("expected API error");
        handle.join().expect("response sequence server thread");

        assert_eq!(error.status(), Some(503));
        assert_eq!(error.request_id(), Some("req_post_123"));
    }

    #[tokio::test]
    async fn api_error_preserves_header_request_id_for_delete_errors() {
        let body = r#"{"message":"not found"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\nrequest-id: req_delete_123\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, _rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.delete_api_key("key_missing").await.expect_err("expected API error");
        handle.join().expect("response sequence server thread");

        assert_eq!(error.status(), Some(404));
        assert_eq!(error.request_id(), Some("req_delete_123"));
        assert_eq!(error.message(), Some("not found"));
    }

    #[tokio::test]
    async fn api_error_keeps_public_variant_match_backward_compatible() {
        let body = r#"{"errorCode":"rate_limited","detail":"retry later","request_id":"req_body_456"}"#;
        let response = format!(
            "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.me().await.expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(429));
        assert_eq!(error.request_id(), Some("req_body_456"));
        assert_eq!(error.code(), Some("rate_limited"));
        assert_eq!(error.message(), Some("retry later"));
        match error {
            SecApiError::Api { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(body["errorCode"], "rate_limited");
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn api_error_extracts_nested_error_object_metadata() {
        let body = r#"{"requestId":"req_nested_789","error":{"code":"mcp_tool_failed","message":"hosted tool failed"}}"#;
        let response = format!(
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client
            .call_mcp_tool("filings.latest", &[], None)
            .await
            .expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(502));
        assert_eq!(error.request_id(), Some("req_nested_789"));
        assert_eq!(error.code(), Some("mcp_tool_failed"));
        assert_eq!(error.message(), Some("hosted tool failed"));
    }

    #[tokio::test]
    async fn api_error_preserves_plain_text_body() {
        let body = "upstream unavailable";
        let response = format!(
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let (base_url, handle) = response_server(response);
        let client = SecApiClient::new(None)
            .with_base_url(base_url)
            .without_retries();

        let error = client.health().await.expect_err("expected API error");
        handle.join().expect("response server thread");

        assert_eq!(error.status(), Some(502));
        assert_eq!(error.body(), Some(&Value::String("upstream unavailable".to_string())));
        assert_eq!(error.request_id(), None);
        assert_eq!(error.message(), None);
    }

    #[tokio::test]
    async fn filing_helpers_escape_dynamic_path_segments() {
        let (base_url, rx, handle) = capture_server(3);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.latest_section("item/1a risk", &[("ticker", "AAPL")]).await.unwrap();
        client.filing_by_accession("0000320193/25 000079", &[("view", "agent")]).await.unwrap();
        client
            .filing_section_by_accession(
                "0000320193/25 000079",
                "item/7 md&a",
                &[("mode", "compact")],
            )
            .await
            .unwrap();

        let targets: Vec<String> = (0..3)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("capture server thread");

        assert_eq!(targets[0], "/v1/filings/latest/sections/item%2F1a%20risk?ticker=AAPL");
        assert_eq!(targets[1], "/v1/filings/0000320193%2F25%20000079?view=agent");
        assert_eq!(
            targets[2],
            "/v1/filings/0000320193%2F25%20000079/sections/item%2F7%20md%26a?mode=compact"
        );
    }

    #[tokio::test]
    async fn embed_situation_helpers_route_to_public_surface() {
        let (base_url, rx, handle) = capture_server(4);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.embed_situations(&[("limit", "20"), ("tickers", "AAPL")]).await.unwrap();
        client.embed_situation("sit/with spaces", &[]).await.unwrap();
        client.embed_situation_issues(&[("limit", "12")]).await.unwrap();
        client.embed_situation_issue("special/situations digest 22").await.unwrap();

        let targets: Vec<String> = (0..4)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("capture server thread");

        assert_eq!(targets[0], "/v1/embed/situations?limit=20&tickers=AAPL");
        assert_eq!(targets[1], "/v1/embed/situations/sit%2Fwith%20spaces");
        assert_eq!(targets[2], "/v1/embed/situations/issues?limit=12");
        assert_eq!(
            targets[3],
            "/v1/embed/situations/issues/special%2Fsituations%20digest%2022"
        );
    }

    #[tokio::test]
    async fn paid_situation_helpers_route_to_authenticated_surface() {
        let (base_url, rx, handle) = capture_server(9);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.list_situations(&[("types", "merger,tender_offer"), ("limit", "20")]).await.unwrap();
        client.get_situation("sit/with spaces", &[("enrich", "false")]).await.unwrap();
        client.situations_feed(&[("tickers", "AAPL,MSFT")]).await.unwrap();
        client.situations_issues(&[("limit", "12")]).await.unwrap();
        client.situation_issue("special/situations digest 22", &[]).await.unwrap();
        client.situations_calendar(&[("statuses", "pending")]).await.unwrap();
        client.situations_stats(&[("window", "30d")]).await.unwrap();
        client.export_situation("sit/with spaces", &[]).await.unwrap();
        client.underwrite_situation("sit/with spaces", &[]).await.unwrap();

        let targets: Vec<String> = (0..9)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("capture server thread");

        assert_eq!(targets[0], "/v1/situations?types=merger%2Ctender_offer&limit=20");
        assert_eq!(targets[1], "/v1/situations/sit%2Fwith%20spaces?enrich=false");
        assert_eq!(targets[2], "/v1/situations/feed?tickers=AAPL%2CMSFT");
        assert_eq!(targets[3], "/v1/situations/issues?limit=12");
        assert_eq!(targets[4], "/v1/situations/issues/special%2Fsituations%20digest%2022");
        assert_eq!(targets[5], "/v1/situations/calendar?statuses=pending");
        assert_eq!(targets[6], "/v1/situations/stats?window=30d");
        assert_eq!(targets[7], "/v1/situations/sit%2Fwith%20spaces/export");
        assert_eq!(targets[8], "/v1/situations/sit%2Fwith%20spaces/underwriting-pack");
    }

    #[tokio::test]
    async fn watch_situations_uses_monitor_substrate() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "content-type: application/json\r\n",
            "content-length: 11\r\n",
            "connection: close\r\n",
            "\r\n",
            "{\"ok\":true}"
        );
        let (base_url, rx, handle) = raw_capture_server(response);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client
            .situations()
            .watch(
                &json!({"types": ["merger"], "tickers": [" AAPL "]}),
                SituationWatchDelivery::Email(" desk@example.com ".to_string()),
                Some(" Deals "),
                Some("2026-07-13T00:00:00Z"),
            )
            .await
            .unwrap();

        let raw_request = rx.recv_timeout(Duration::from_secs(2)).expect("raw request");
        handle.join().expect("raw capture server thread");

        assert!(raw_request.starts_with("POST /v1/monitors HTTP/1.1"));
        assert!(raw_request.contains(r#""query":"situations.watch""#));
        assert!(raw_request.contains(r#""searchMode":"situation""#));
        assert!(raw_request.contains(r#""name":"Deals""#));
        assert!(raw_request.contains(r#""delivery":{"config":{"to":"desk@example.com"},"type":"email"}"#));
        assert!(raw_request.contains(r#""filters":{"tickers":["AAPL"],"types":["merger"]}"#));
    }

    #[tokio::test]
    async fn watch_situations_validates_filters_and_delivery() {
        let (base_url, _rx, handle) = capture_server(0);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let missing_filters = client
            .watch_situations(&json!({}), SituationWatchDelivery::OrganizationWebhook, None, None)
            .await
            .expect_err("expected missing filter error");
        assert!(missing_filters.to_string().contains("at least one"));

        let invalid_type = client
            .watch_situations(&json!({"types": ["not-a-type"]}), SituationWatchDelivery::OrganizationWebhook, None, None)
            .await
            .expect_err("expected invalid type error");
        assert!(invalid_type.to_string().contains("canonical situation types"));

        let invalid_subtype = client
            .watch_situations(&json!({"subtypes": ["not-a-subtype"]}), SituationWatchDelivery::OrganizationWebhook, None, None)
            .await
            .expect_err("expected invalid subtype error");
        assert!(invalid_subtype.to_string().contains("canonical situation subtypes"));

        let invalid_situation_id = client
            .watch_situations(&json!({"situationIds": ["not-a-situation-id"]}), SituationWatchDelivery::OrganizationWebhook, None, None)
            .await
            .expect_err("expected invalid situation id error");
        assert!(invalid_situation_id.to_string().contains("canonical ids"));

        let invalid_delivery = client
            .watch_situations(&json!({"types": ["merger"]}), SituationWatchDelivery::Email(" ".to_string()), None, None)
            .await
            .expect_err("expected invalid delivery error");
        assert!(invalid_delivery.to_string().contains("non-empty email"));

        handle.join().expect("zero-request server thread");
    }

    #[test]
    fn typed_request_builders_omit_blank_values_and_allow_extra_overrides() {
        let request = LatestFilingRequest::new()
            .ticker(" AAPL ")
            .form("10-K")
            .view(ResponseView::Agent)
            .extra("view", "compact")
            .extra("ignored", " ");
        let params = request.params();
        let pairs = params.as_pairs();

        assert_eq!(pairs, vec![("ticker", "AAPL"), ("form", "10-K"), ("view", "compact")]);
        assert!(!params.is_empty());
    }

    #[tokio::test]
    async fn typed_request_builders_drive_high_frequency_routes() {
        let (base_url, rx, handle) = capture_server(4);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let entity = ResolveEntityRequest::new()
            .ticker("AAPL")
            .view(ResponseView::Agent);
        client.resolve_entity_with(&entity).await.unwrap();

        let filing = LatestFilingRequest::new()
            .ticker("AAPL")
            .form("10-K")
            .filing_year("2025")
            .view(ResponseView::Agent);
        client.latest_filing_with(&filing).await.unwrap();

        let section = LatestSectionRequest::new("item/1a risk")
            .ticker("AAPL")
            .form("10-K")
            .mode("compact");
        client.latest_section_with(&section).await.unwrap();

        let semantic = SemanticSearchRequest::new("Risk Factors")
            .ticker("AMD")
            .form("10-K")
            .filing_year("2024")
            .mode(SemanticSearchMode::Semantic)
            .limit("3")
            .view(ResponseView::Agent);
        client.semantic_search_with(&semantic).await.unwrap();

        let targets: Vec<String> = (0..4)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("capture server thread");

        assert_eq!(targets[0], "/v1/entities/resolve?ticker=AAPL&view=agent");
        assert_eq!(
            targets[1],
            "/v1/filings/latest?ticker=AAPL&form=10-K&filing_year=2025&view=agent"
        );
        assert_eq!(
            targets[2],
            "/v1/filings/latest/sections/item%2F1a%20risk?ticker=AAPL&form=10-K&mode=compact"
        );
        assert_eq!(
            targets[3],
            "/v1/search/semantic?q=Risk+Factors&ticker=AMD&form=10-K&filing_year=2024&mode=semantic&limit=3&view=agent"
        );
    }


    #[tokio::test]
    async fn grouped_services_delegate_to_flat_client_methods() {
        let (base_url, rx, handle) = capture_server(6);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.entities().resolve(&[("ticker", "AAPL")]).await.unwrap();
        let latest_filing = client.filings().latest(&[("ticker", "AAPL"), ("form", "10-K")]);
        latest_filing.await.unwrap();
        client
            .sections()
            .latest("item_1a", &[("ticker", "AAPL"), ("form", "10-K"), ("mode", "compact")])
            .await
            .unwrap();
        client
            .search()
            .semantic(&[
                ("q", "supply chain risk"),
                ("ticker", "AAPL"),
                ("mode", "hybrid"),
                ("view", "agent"),
            ])
            .await
            .unwrap();
        client
            .factors()
            .history("VALUE", &[("range", "1y"), ("response_mode", "compact"), ("include", "trust,series")])
            .await
            .unwrap();
        client
            .factors()
            .dashboard(&[
                ("country", "US"),
                ("category", "style"),
                ("ticker", "AAPL"),
                ("response_mode", "compact"),
            ])
            .await
            .unwrap();

        let targets: Vec<String> = (0..6)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("capture server thread");

        assert_eq!(targets[0], "/v1/entities/resolve?ticker=AAPL");
        assert_eq!(targets[1], "/v1/filings/latest?ticker=AAPL&form=10-K");
        assert_eq!(
            targets[2],
            "/v1/filings/latest/sections/item_1a?ticker=AAPL&form=10-K&mode=compact"
        );
        assert_eq!(
            targets[3],
            "/v1/search/semantic?q=supply+chain+risk&ticker=AAPL&mode=hybrid&view=agent"
        );
        assert_eq!(
            targets[4],
            "/v1/factors/history/VALUE?range=1y&response_mode=compact&include=trust%2Cseries"
        );
        assert_eq!(
            targets[5],
            "/v1/factors/dashboard?country=US&category=style&ticker=AAPL&response_mode=compact"
        );
    }

    #[tokio::test]
    async fn paginate_filings_follows_next_cursor() {
        let first = json_response(r#"{"object":"list","data":[{"accessionNumber":"0001"},{"accessionNumber":"0002"}],"nextCursor":"cur_2"}"#);
        let second = json_response(r#"{"object":"list","data":[{"accessionNumber":"0003"}],"nextCursor":null}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![first, second]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_filings(&[("ticker", "AAPL"), ("form", "10-K"), ("limit", "2")]);
        let mut accessions = Vec::new();
        while let Some(item) = iterator.next().await.unwrap() {
            accessions.push(item["accessionNumber"].as_str().unwrap().to_string());
        }

        let targets: Vec<String> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(2)).expect("request target"))
            .collect();
        handle.join().expect("response sequence server thread");

        assert_eq!(accessions, vec!["0001", "0002", "0003"]);
        assert_eq!(
            targets,
            vec![
                "/v1/filings?ticker=AAPL&form=10-K&limit=2",
                "/v1/filings?ticker=AAPL&form=10-K&limit=2&cursor=cur_2",
            ]
        );
        assert_eq!(iterator.page_count(), 2);
        assert_eq!(iterator.item_count(), 3);
    }

    #[tokio::test]
    async fn paginate_sections_stops_when_has_more_is_false() {
        let response = json_response(r#"{"object":"list","hasMore":false,"nextCursor":"ignored","data":[{"key":"item_1a"}]}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_sections(&[("ticker", "AAPL"), ("q", "risk"), ("limit", "1")]);
        let first = iterator.next().await.unwrap().expect("first item");
        let second = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["key"], "item_1a");
        assert!(second.is_none());
        assert_eq!(target, "/v1/sections/search?ticker=AAPL&q=risk&limit=1");
    }

    #[tokio::test]
    async fn paginate_sections_stops_when_has_more_snake_case_is_false() {
        let response = json_response(r#"{"object":"list","has_more":false,"next_cursor":"ignored","data":[{"key":"item_1a"}]}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_sections(&[("ticker", "AAPL"), ("q", "risk"), ("limit", "1")]);
        let first = iterator.next().await.unwrap().expect("first item");
        let second = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["key"], "item_1a");
        assert!(second.is_none());
        assert_eq!(target, "/v1/sections/search?ticker=AAPL&q=risk&limit=1");
    }

    #[tokio::test]
    async fn paginate_filings_stops_on_empty_page_even_with_fresh_cursor() {
        let response = json_response(r#"{"object":"list","data":[],"nextCursor":"cur_fresh"}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_filings(&[("ticker", "AAPL"), ("limit", "1")]);
        let first = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert!(first.is_none());
        assert_eq!(target, "/v1/filings?ticker=AAPL&limit=1");
        assert_eq!(iterator.page_count(), 1);
        assert_eq!(iterator.item_count(), 0);
    }

    #[tokio::test]
    async fn pagination_options_cap_items_within_a_page() {
        let response = json_response(r#"{"object":"list","data":[{"id":"filing_1"},{"id":"filing_2"}],"nextCursor":"cur_2"}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_filings_with_options(
            &[("ticker", "AAPL"), ("limit", "2")],
            PaginationOptions::default().max_items(1),
        );
        let first = iterator.next().await.unwrap().expect("first item");
        let second = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["id"], "filing_1");
        assert!(second.is_none());
        assert_eq!(target, "/v1/filings?ticker=AAPL&limit=2");
        assert_eq!(iterator.item_count(), 1);
    }

    #[tokio::test]
    async fn pagination_options_cap_pages_without_extra_fetch() {
        let response = json_response(r#"{"object":"list","data":[{"id":"entity_1"}],"nextCursor":"cur_2"}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_entities_with_options(
            &[("q", "apple"), ("limit", "1")],
            PaginationOptions::default().max_pages(1),
        );
        let first = iterator.next().await.unwrap().expect("first item");
        let second = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["id"], "entity_1");
        assert!(second.is_none());
        assert_eq!(target, "/v1/entities?q=apple&limit=1");
        assert_eq!(iterator.page_count(), 1);
        assert_eq!(iterator.item_count(), 1);
    }

    #[tokio::test]
    async fn custom_paginate_normalizes_leading_slash() {
        let response = json_response(r#"{"object":"list","data":[{"id":"entity_1"}]}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate("v1/entities", &[("q", "apple")], PaginationOptions::default());
        let first = iterator.next().await.unwrap().expect("first item");
        let second = iterator.next().await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["id"], "entity_1");
        assert!(second.is_none());
        assert_eq!(target, "/v1/entities?q=apple");
    }

    #[tokio::test]
    async fn repeated_cursor_yields_current_items_before_error() {
        let response = json_response(r#"{"object":"list","data":[{"id":"filing_1"}],"nextCursor":"cur_repeat"}"#);
        let (base_url, rx, handle) = response_sequence_server(vec![response]);
        let client = SecApiClient::new(None).with_base_url(base_url);

        let mut iterator = client.paginate_filings(&[("cursor", "cur_repeat"), ("limit", "1")]);
        let first = iterator.next().await.unwrap().expect("first item");
        let error = iterator.next().await.expect_err("expected repeated cursor error");

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("response sequence server thread");

        assert_eq!(first["id"], "filing_1");
        assert_eq!(target, "/v1/filings?cursor=cur_repeat&limit=1");
        match error {
            PageIteratorError::Pagination { message } => {
                assert!(message.contains("pagination cursor repeated"));
                assert!(message.contains("cur_repeat"));
            }
            other => panic!("expected pagination error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stock_loadings_escapes_ticker_path_segment() {
        let (base_url, rx, handle) = capture_server(1);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.stock_loadings("BRK/B class", &[("view", "agent")]).await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("capture server thread");

        assert_eq!(target, "/v1/stocks/BRK%2FB%20class/loadings?view=agent");
    }

    #[tokio::test]
    async fn model_portfolio_factor_view_escapes_portfolio_id_path_segment() {
        let (base_url, rx, handle) = capture_server(1);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client
            .model_portfolio_factor_view("portfolio/team alpha", &[("view", "agent")])
            .await
            .unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("capture server thread");

        assert_eq!(
            target,
            "/v1/model-portfolios/portfolio%2Fteam%20alpha/factor-view?view=agent"
        );
    }

    #[tokio::test]
    async fn delete_api_key_escapes_key_id_path_segment() {
        let (base_url, rx, handle) = capture_server(1);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client.delete_api_key("key/team alpha").await.unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("capture server thread");

        assert_eq!(target, "/v1/api_keys/key%2Fteam%20alpha");
    }

    #[tokio::test]
    async fn dilution_event_detail_escapes_event_id_path_segment() {
        let (base_url, rx, handle) = capture_server(1);
        let client = SecApiClient::new(None).with_base_url(base_url);

        client
            .dilution_event_detail("event/team alpha", &[("view", "agent")])
            .await
            .unwrap();

        let target = rx.recv_timeout(Duration::from_secs(2)).expect("request target");
        handle.join().expect("capture server thread");

        assert_eq!(
            target,
            "/v1/dilution/events/event%2Fteam%20alpha?view=agent"
        );
    }

    #[test]
    fn mcp_tool_call_body_builds_tools_call_envelope() {
        let body = mcp_tool_call_body(
            "filings.latest",
            &[("ticker", json!("AAPL")), ("form", json!("10-K"))],
            Some("agent-test"),
        );

        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], "agent-test");
        assert_eq!(body["method"], "tools/call");
        assert_eq!(body["params"]["name"], "filings.latest");
        assert_eq!(body["params"]["arguments"]["ticker"], "AAPL");
        assert_eq!(body["params"]["arguments"]["form"], "10-K");
    }

    #[test]
    fn mcp_tool_call_body_defaults_empty_arguments_to_object() {
        let body = mcp_tool_call_body("tools/list", &[], None);

        assert_eq!(body["id"], "secapi-rust");
        assert_eq!(body["params"]["arguments"], json!({}));
    }
}
