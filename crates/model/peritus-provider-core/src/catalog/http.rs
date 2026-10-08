//! Bounded authenticated GET discovery with same-origin token pagination.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::{DiscoveredModel, parse, unavailable};
use crate::{
    CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest, HttpTransport,
    ProviderCoreError,
};

/// The actual catalog wire family, independent of model-name conventions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogDialect {
    /// OpenAI-style `data` array, also used by compatible endpoints.
    OpenAi,
    /// Fireworks management catalog with model names and token pagination.
    Fireworks,
    /// Together AI exposes a top-level model array.
    Together,
    /// Anthropic `data`, `has_more`, and `last_id` pagination.
    Anthropic,
    /// Stable-v1 Google `models` and `nextPageToken` pagination.
    GoogleV1,
}

/// Result of committing one complete physical provider page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogProgress {
    /// The provider supplied an owned continuation cursor.
    Incomplete,
    /// The provider supplied no continuation cursor.
    Complete,
}

/// Resumable, same-endpoint HTTP catalog ownership.
///
/// Every successful page is parsed and conflict-checked before its inventory and continuation are
/// committed. A failed or cancelled resume retains all earlier pages and retries the same cursor.
pub struct HttpCatalogDiscovery {
    endpoint: Endpoint,
    dialect: CatalogDialect,
    models: BTreeMap<String, DiscoveredModel>,
    cursors: BTreeSet<String>,
    next_cursor: Option<String>,
    started: bool,
    complete: bool,
}

impl HttpCatalogDiscovery {
    /// Starts discovery at one already reviewed catalog endpoint.
    #[must_use]
    pub fn new(endpoint: Endpoint, dialect: CatalogDialect) -> Self {
        Self {
            endpoint,
            dialect,
            models: BTreeMap::new(),
            cursors: BTreeSet::new(),
            next_cursor: None,
            started: false,
            complete: false,
        }
    }

    /// Returns whether the provider has ended pagination.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    /// Borrows the exact successfully committed inventory prefix.
    pub fn models(&self) -> impl Iterator<Item = &DiscoveredModel> {
        self.models.values()
    }

    /// Borrows the provider cursor that will be retried by the next resume.
    #[must_use]
    pub fn continuation(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    /// Transfers a complete exact inventory in provider-ID order.
    ///
    /// # Errors
    /// Rejects extraction before the provider ends pagination.
    pub fn into_models(self) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
        if !self.complete {
            return Err(unavailable("model catalog extraction preceded pagination completion"));
        }
        Ok(self.models.into_values().collect())
    }

    /// Fetches and atomically commits at most one physical provider page.
    ///
    /// # Errors
    /// Returns authentication/status, malformed-data, cancellation, cursor, or physical response
    /// failures without discarding previously committed pages.
    pub async fn resume_page(
        &mut self,
        transport: &dyn HttpTransport,
        headers: &(dyn Fn() -> Result<HttpHeaders, ProviderCoreError> + Sync),
        limits: HttpLimits,
        cancellation: &CancellationToken,
    ) -> Result<CatalogProgress, ProviderCoreError> {
        if self.complete {
            return Ok(CatalogProgress::Complete);
        }
        if cancellation.is_cancelled() {
            return Err(ProviderCoreError::cancelled("model_catalog"));
        }
        let endpoint = if self.started {
            let cursor = self.next_cursor.as_deref().ok_or_else(|| {
                unavailable("model catalog continuation state is incomplete")
            })?;
            Endpoint::new(next_page(&self.endpoint, self.dialect, cursor).to_string())?
        } else {
            self.endpoint.clone()
        };
        let request = HttpRequest::new(
            HttpMethod::Get,
            endpoint,
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
            let next = bytes
                .len()
                .checked_add(chunk.len())
                .ok_or_else(|| unavailable("model catalog page byte count overflowed"))?;
            if next > limits.max_response_body_bytes() {
                return Err(unavailable("model catalog page exceeds its HTTP response bound"));
            }
            bytes
                .try_reserve(chunk.len())
                .map_err(|_| unavailable("model catalog page capacity is unavailable"))?;
            bytes.extend_from_slice(&chunk);
        }
        let page: Value = serde_json::from_slice(&bytes)
            .map_err(|_| unavailable("provider returned an invalid model catalog"))?;
        let values = if self.dialect == CatalogDialect::Together {
            Some(&page)
        } else {
            page.get(if matches!(self.dialect, CatalogDialect::GoogleV1 | CatalogDialect::Fireworks) {
                "models"
            } else {
                "data"
            })
        }
        .and_then(Value::as_array)
        .ok_or_else(|| unavailable("provider catalog is missing its model array"))?;
        let mut page_models = BTreeMap::new();
        for value in values {
            if self.dialect == CatalogDialect::Fireworks
                && (value.get("supportsServerless").and_then(Value::as_bool) == Some(false)
                    || !value.get("conversationConfig").is_some_and(Value::is_object))
            {
                continue;
            }
            let model = parse::model(
                value,
                matches!(self.dialect, CatalogDialect::GoogleV1 | CatalogDialect::Fireworks),
            )?;
            if let Some(prior) = page_models.insert(model.id.as_str().to_owned(), model.clone())
                && prior != model
            {
                return Err(unavailable("provider returned conflicting duplicate model metadata"));
            }
            if self
                .models
                .get(model.id.as_str())
                .is_some_and(|prior| prior != &model)
            {
                return Err(unavailable("provider returned conflicting duplicate model metadata"));
            }
        }
        let cursor = if matches!(self.dialect, CatalogDialect::GoogleV1 | CatalogDialect::Fireworks)
        {
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
        let cursor = cursor
            .map(|value| {
                if value.len() > 4096 || self.cursors.contains(value) {
                    return Err(unavailable(
                        "model catalog pagination cursor is repeated or oversized",
                    ));
                }
                let mut owned = String::new();
                owned
                    .try_reserve_exact(value.len())
                    .map_err(|_| unavailable("model catalog cursor capacity is unavailable"))?;
                owned.push_str(value);
                Ok(owned)
            })
            .transpose()?;
        for (id, model) in page_models {
            self.models.entry(id).or_insert(model);
        }
        self.started = true;
        match cursor {
            Some(cursor) => {
                self.cursors.insert(cursor.clone());
                self.next_cursor = Some(cursor);
                Ok(CatalogProgress::Incomplete)
            }
            None => {
                self.next_cursor = None;
                self.complete = true;
                Ok(CatalogProgress::Complete)
            }
        }
    }
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
    let mut discovery = HttpCatalogDiscovery::new(endpoint.clone(), dialect);
    while !discovery.is_complete() {
        let _ = discovery.resume_page(transport, headers, limits, cancellation).await?;
    }
    discovery.into_models()
}

fn next_page(endpoint: &Endpoint, dialect: CatalogDialect, cursor: &str) -> url::Url {
    let mut next = endpoint.url().clone();
    let cursor_name =
        if matches!(dialect, CatalogDialect::GoogleV1 | CatalogDialect::Fireworks) {
            "pageToken"
        } else {
            "after_id"
        };
    let retained: Vec<(String, String)> = next
        .query_pairs()
        .filter(|(name, _)| name != cursor_name)
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    next.set_query(None);
    next.query_pairs_mut()
        .extend_pairs(retained.iter().map(|(name, value)| (name.as_str(), value.as_str())))
        .append_pair(cursor_name, cursor);
    next
}
