//! Bounded authenticated GET discovery with same-origin token pagination.

use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use super::{DiscoveredModel, MAX_CATALOG_MODELS, parse, unavailable};
use crate::{
    CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest, HttpTransport,
    ProviderCoreError,
};

const MAX_PAGES: usize = 16;
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// The actual catalog wire family, independent of model-name conventions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogDialect {
    /// OpenAI-style `data` array, also used by compatible endpoints.
    OpenAi,
    /// Anthropic `data`, `has_more`, and `last_id` pagination.
    Anthropic,
    /// Google `models` and `nextPageToken` pagination.
    Google,
}

/// Fetches a complete bounded catalog using the supplied endpoint and freshly resolved headers.
///
/// The endpoint must already point to the provider's models route. Pagination changes only a
/// cursor query on that same URL. No response URL is followed and no inference is performed.
///
/// # Errors
/// Returns authentication/status, malformed-data, cancellation, size, or deadline failures.
pub async fn discover_http_models(
    transport: &dyn HttpTransport,
    endpoint: &Endpoint,
    dialect: CatalogDialect,
    headers: &(dyn Fn() -> Result<HttpHeaders, ProviderCoreError> + Sync),
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    tokio::time::timeout(
        Duration::from_secs(30),
        discover(transport, endpoint, dialect, headers, limits, cancellation),
    )
    .await
    .map_err(|_| unavailable("model discovery timed out; retry or select an explicit model ID"))?
}

async fn discover(
    transport: &dyn HttpTransport,
    endpoint: &Endpoint,
    dialect: CatalogDialect,
    headers: &(dyn Fn() -> Result<HttpHeaders, ProviderCoreError> + Sync),
    limits: HttpLimits,
    cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let mut next = endpoint.url().clone();
    let mut cursors = BTreeSet::new();
    let mut models = BTreeMap::new();
    let mut total = 0_usize;
    for _ in 0..MAX_PAGES {
        let request = HttpRequest::new(
            HttpMethod::Get,
            Endpoint::new(next.to_string())?,
            headers()?,
            Vec::new(),
            limits,
        )?;
        let response = transport.send(request, cancellation).await?;
        if !response.status().is_success() {
            return Err(unavailable(match response.status().as_u16() {
                401 | 403 => {
                    "provider denied model discovery; check this endpoint's authentication"
                }
                404 | 405 | 501 => {
                    "provider does not expose this model catalog; select an explicit model ID"
                }
                429 => "provider rate-limited model discovery; retry later",
                _ => "provider model discovery failed; no fallback model list was substituted",
            }));
        }
        let (_, _, mut body) = response.into_parts();
        let mut bytes = Vec::new();
        while let Some(chunk) = body.next(cancellation).await? {
            total = total.saturating_add(chunk.len());
            if bytes.len().saturating_add(chunk.len()) > MAX_PAGE_BYTES || total > MAX_TOTAL_BYTES {
                return Err(unavailable("model catalog response exceeds its byte bound"));
            }
            bytes.extend_from_slice(&chunk);
        }
        let page: Value = serde_json::from_slice(&bytes)
            .map_err(|_| unavailable("provider returned an invalid model catalog"))?;
        let values = page
            .get(if dialect == CatalogDialect::Google { "models" } else { "data" })
            .and_then(Value::as_array)
            .ok_or_else(|| unavailable("provider catalog is missing its model array"))?;
        if values.len() > MAX_CATALOG_MODELS {
            return Err(unavailable("model catalog exceeds its entry bound"));
        }
        for value in values {
            let model = parse::model(value, dialect == CatalogDialect::Google)?;
            if let Some(prior) = models.insert(model.id.as_str().to_owned(), model.clone())
                && prior != model
            {
                return Err(unavailable("provider returned conflicting duplicate model metadata"));
            }
            if models.len() > MAX_CATALOG_MODELS {
                return Err(unavailable("model catalog exceeds its entry bound"));
            }
        }
        let cursor = if dialect == CatalogDialect::Google {
            page.get("nextPageToken").and_then(Value::as_str).filter(|value| !value.is_empty())
        } else if page.get("has_more").and_then(Value::as_bool) == Some(true) {
            Some(
                page.get("last_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        unavailable("paginated model catalog has no continuation cursor")
                    })?,
            )
        } else {
            None
        };
        let Some(cursor) = cursor else {
            return Ok(models.into_values().collect());
        };
        if cursor.len() > 4096 || !cursors.insert(cursor.to_owned()) {
            return Err(unavailable("model catalog pagination cursor is repeated or oversized"));
        }
        next = endpoint.url().clone();
        next.query_pairs_mut().append_pair(
            if dialect == CatalogDialect::Google { "pageToken" } else { "after_id" },
            cursor,
        );
    }
    Err(unavailable("model catalog exceeds its page bound; no partial catalog was substituted"))
}
