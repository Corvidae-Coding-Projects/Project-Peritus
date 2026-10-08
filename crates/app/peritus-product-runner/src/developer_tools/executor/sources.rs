//! Bounded source catalogs and range reads supplied by the governing conversation port.

use super::{WorkspaceDeveloperTools, object, tool};
use crate::{ContextSourceKind, ContextSourcePage, ContextSourceSlice};
use peritus_agent::DeveloperLoopError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use crate::developer_tools::reference::ExplicitReferences;

const AUTHORITY_BOUNDARY_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Default)]
pub(in crate::developer_tools) struct RequestSourceProgress {
    catalog_binding: Option<[u8; 32]>,
    catalog_authority: Option<[u8; 32]>,
    verified_authority: Option<[u8; 32]>,
    discovery: RequestCatalogProgress,
    traversal: RequestCatalogProgress,
    pages: BTreeMap<Option<u64>, ContextSourcePage>,
    bodies: BTreeMap<u64, RequestBodyProgress>,
    prior_bodies: Vec<RequestBodyProgress>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum RequestCatalogProgress {
    #[default]
    NotStarted,
    More(u64),
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RequestBodyProgress {
    descriptor: crate::ContextSource,
    frontier: u64,
    complete: bool,
    authority_boundary: String,
    references: ExplicitReferences,
}

impl RequestSourceProgress {
    pub(in crate::developer_tools) fn merge(&mut self, other: &Self) {
        if other.catalog_binding.is_none() {
            return;
        }
        let mut merged = other.clone();
        if self.catalog_binding == other.catalog_binding
            && self.catalog_authority == other.catalog_authority
        {
            for (ordinal, prior) in &self.bodies {
                match merged.bodies.get_mut(ordinal) {
                    Some(body) if same_body(&prior.descriptor, &body.descriptor) => {
                        if prior.frontier > body.frontier {
                            body.clone_from(prior);
                        }
                    }
                    None => {
                        merged.bodies.insert(*ordinal, prior.clone());
                    }
                    Some(_) => return,
                }
            }
            for (cursor, page) in &self.pages {
                if merged.pages.get(cursor).is_some_and(|candidate| candidate != page) {
                    return;
                }
                merged.pages.entry(*cursor).or_insert_with(|| page.clone());
            }
            if catalog_frontier(self.discovery) > catalog_frontier(merged.discovery) {
                merged.discovery = self.discovery;
            }
        }
        merged.refresh_verified_authority();
        *self = merged;
    }

    /// Reconstructs credit only from exact successful host call/result pairs. Older observations
    /// without bindings grant no credit; malformed or stale observations cannot widen authority.
    pub(in crate::developer_tools) fn recover_completed(
        &mut self,
        root: &Path,
        name: &str,
        arguments: &Value,
        result: &Value,
    ) {
        let mut candidate = self.clone();
        let recovered = (|| {
            let authority = bound_digest(result, "authorityBinding")?;
            let catalog = bound_digest(result, "catalogBinding")?;
            match name {
                "request_sources" => {
                    let after = decimal_argument(arguments, "after")?;
                    candidate.expect_page(after, authority, catalog)?;
                    let page = recovered_page(after, result)?;
                    candidate.observe_page(after, &page)?;
                }
                "request_source_read" => {
                    if candidate.catalog_authority != Some(authority)
                        || candidate.catalog_binding != Some(catalog)
                    {
                        return Err(tool("recovered request slice has no exact bound catalog"));
                    }
                    let source = required_cursor(arguments, "source")?;
                    let offset = required_cursor(arguments, "offset")?;
                    candidate.expect_slice(source, offset)?;
                    if required_cursor(result, "source")? != source
                        || required_cursor(result, "offset")? != offset
                    {
                        return Err(tool("recovered request slice differs from its call"));
                    }
                    let descriptor = &candidate.bodies.get(&source)
                        .ok_or_else(|| tool("recovered request slice has no descriptor"))?
                        .descriptor;
                    let text = result.get("text").and_then(Value::as_str)
                        .ok_or_else(|| tool("recovered request slice has no text"))?;
                    let next = nullable_cursor(result, "next")?;
                    if result.get("complete").and_then(Value::as_bool) != Some(next.is_none()) {
                        return Err(tool("recovered request slice has an invalid completion flag"));
                    }
                    let slice = ContextSourceSlice::new(descriptor, offset, text.to_owned(), next)
                        .map_err(tool)?;
                    candidate.observe_slice(&slice, root)?;
                }
                _ => return Ok(()),
            }
            Ok(())
        })();
        if recovered.is_ok() {
            *self = candidate;
        }
    }

    pub(super) fn is_complete_at(&self, binding: [u8; 32]) -> bool {
        self.verified_authority == Some(binding)
    }

    pub(super) fn required_tool_at(&self, authority: [u8; 32]) -> Option<&'static str> {
        if self.is_complete_at(authority) {
            None
        } else if self.catalog_authority == Some(authority)
            && self.discovery == RequestCatalogProgress::Complete
        {
            Some("request_source_read")
        } else {
            Some("request_sources")
        }
    }

    fn expect_page(
        &mut self,
        after: Option<u64>,
        authority: [u8; 32],
        catalog: [u8; 32],
    ) -> Result<(), DeveloperLoopError> {
        if self.catalog_binding != Some(catalog) || self.catalog_authority != Some(authority) {
            if after.is_some() {
                return Err(tool(
                    "the admitted request-source catalog changed; restart request_sources without an after cursor",
                ));
            }
            self.reset_catalog(authority, catalog);
        }
        if after.is_none() {
            self.traversal = RequestCatalogProgress::NotStarted;
        }
        let expected = match self.traversal {
            RequestCatalogProgress::NotStarted => None,
            RequestCatalogProgress::More(after) => Some(after),
            RequestCatalogProgress::Complete => {
                return Err(tool(
                    "the authoritative request-source catalog traversal is complete; restart without an after cursor",
                ));
            }
        };
        if after != expected {
            return Err(tool("request_sources must follow the exact next catalog cursor"));
        }
        Ok(())
    }

    fn reset_catalog(&mut self, authority: [u8; 32], catalog: [u8; 32]) {
        self.prior_bodies = std::mem::take(&mut self.bodies)
            .into_values()
            .filter(|body| body.descriptor.kind() == ContextSourceKind::UserRequest)
            .collect();
        self.pages.clear();
        self.discovery = RequestCatalogProgress::NotStarted;
        self.traversal = RequestCatalogProgress::NotStarted;
        self.catalog_binding = Some(catalog);
        self.catalog_authority = Some(authority);
    }

    fn refresh_verified_authority(&mut self) {
        if self.discovery == RequestCatalogProgress::Complete
            && self
                .bodies
                .values()
                .filter(|body| body.descriptor.kind() == ContextSourceKind::UserRequest)
                .all(|body| body.complete)
        {
            self.verified_authority = self.catalog_authority;
        }
    }

    fn observe_page(
        &mut self,
        after: Option<u64>,
        page: &ContextSourcePage,
    ) -> Result<(), DeveloperLoopError> {
        if let Some(bound) = self.pages.get(&after) {
            if bound != page {
                return Err(tool("host substituted an authoritative request-source page"));
            }
        } else {
            let expected = match self.discovery {
                RequestCatalogProgress::NotStarted => None,
                RequestCatalogProgress::More(after) => Some(after),
                RequestCatalogProgress::Complete => {
                    return Err(tool("host extended a completed authoritative source catalog"));
                }
            };
            if after != expected {
                return Err(tool("host returned an unbound authoritative source catalog page"));
            }
            for source in page.sources() {
                if !matches!(
                    source.kind(),
                    ContextSourceKind::UserRequest | ContextSourceKind::AssistantHistory
                ) {
                    return Err(tool(
                        "host returned an untyped body in the governing conversation catalog",
                    ));
                }
                let mut progress = RequestBodyProgress {
                    descriptor: source.clone(),
                    frontier: if source.kind() == ContextSourceKind::UserRequest
                        && !source.requires_read()
                    {
                        source.bytes()
                    } else {
                        0
                    },
                    complete: source.bytes() == 0
                        || (source.kind() == ContextSourceKind::UserRequest
                            && !source.requires_read()),
                    authority_boundary: String::new(),
                    references: ExplicitReferences::default(),
                };
                if source.kind() == ContextSourceKind::UserRequest
                    && let Some(index) = self
                        .prior_bodies
                        .iter()
                        .position(|prior| same_body(&prior.descriptor, source))
                {
                    let prior = self.prior_bodies.remove(index);
                    if prior.frontier > progress.frontier {
                        progress.frontier = prior.frontier;
                        progress.complete = prior.complete;
                        progress.authority_boundary = prior.authority_boundary;
                    }
                    progress.references = prior.references;
                    progress.access_policy = prior.access_policy;
                }
                if self.bodies.insert(source.ordinal(), progress).is_some()
                {
                    return Err(tool("host repeated an authoritative request-source descriptor"));
                }
            }
            self.pages.insert(after, page.clone());
            self.discovery = page.next().map_or(
                RequestCatalogProgress::Complete,
                RequestCatalogProgress::More,
            );
        }
        self.traversal = page
            .next()
            .map_or(RequestCatalogProgress::Complete, RequestCatalogProgress::More);
        self.refresh_verified_authority();
        Ok(())
    }

    fn expect_slice(&self, source: u64, offset: u64) -> Result<(), DeveloperLoopError> {
        let body = self
            .bodies
            .get(&source)
            .ok_or_else(|| tool("request source was not returned by the enumerated catalog"))?;
        if offset > body.frontier {
            return Err(tool(
                "request_source_read cannot skip beyond the verified byte frontier",
            ));
        }
        if offset == body.frontier && body.complete {
            return Err(tool(
                "the completed authoritative request source can only be reread from an earlier offset",
            ));
        }
        Ok(())
    }

    fn observe_slice(
        &mut self,
        slice: &ContextSourceSlice,
        root: &Path,
    ) -> Result<Option<String>, DeveloperLoopError> {
        let body = self
            .bodies
            .get_mut(&slice.source())
            .ok_or_else(|| tool("request source was not returned by the enumerated catalog"))?;
        if body.descriptor.ordinal() != slice.source() {
            return Err(tool("request source descriptor identity changed"));
        }
        let end = slice
            .offset()
            .checked_add(
                u64::try_from(slice.text().len())
                    .map_err(|_| tool("request source offset overflow"))?,
            )
            .ok_or_else(|| tool("request source offset overflow"))?;
        let expected_next = (end < body.descriptor.bytes()).then_some(end);
        if end > body.descriptor.bytes() || slice.next() != expected_next {
            return Err(tool("request source slice differs from its catalog descriptor"));
        }
        if slice.offset() < body.frontier {
            return Ok(None);
        }
        if slice.offset() != body.frontier {
            return Err(tool("request source slice skipped the verified byte frontier"));
        }
        body.frontier = end;
        body.complete = slice.next().is_none();

        if body.descriptor.kind() == ContextSourceKind::AssistantHistory {
            body.authority_boundary.clear();
            return Ok(None);
        }

        let mut authority = std::mem::take(&mut body.authority_boundary);
        authority.push_str(slice.text());
        body.references.extend_from_task(root, &authority);
        if !body.complete {
            body.authority_boundary = utf8_suffix(&authority, AUTHORITY_BOUNDARY_BYTES);
        }
        self.refresh_verified_authority();
        Ok(Some(authority))
    }
}

fn same_body(left: &crate::ContextSource, right: &crate::ContextSource) -> bool {
    left.kind() == right.kind()
        && left.label() == right.label()
        && left.digest() == right.digest()
        && left.bytes() == right.bytes()
}

fn catalog_frontier(progress: RequestCatalogProgress) -> u64 {
    match progress {
        RequestCatalogProgress::NotStarted => 0,
        RequestCatalogProgress::More(cursor) => cursor,
        RequestCatalogProgress::Complete => u64::MAX,
    }
}

fn bound_value(mut value: Value, authority: [u8; 32], catalog: [u8; 32]) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert("authorityBinding".to_owned(), Value::String(hex_bytes(&authority)));
        object.insert("catalogBinding".to_owned(), Value::String(hex_bytes(&catalog)));
    }
    value
}

fn bound_digest(value: &Value, name: &str) -> Result<[u8; 32], DeveloperLoopError> {
    let text = value.get(name).and_then(Value::as_str)
        .ok_or_else(|| tool(format!("{name} is missing")))?;
    if text.len() != 64
        || !text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(tool(format!("{name} is not an exact digest")));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair).map_err(|_| tool("invalid digest encoding"))?;
        digest[index] = u8::from_str_radix(pair, 16)
            .map_err(|_| tool("invalid digest encoding"))?;
    }
    Ok(digest)
}

fn nullable_cursor(value: &Value, name: &str) -> Result<Option<u64>, DeveloperLoopError> {
    match value.get(name) {
        Some(Value::Null) => Ok(None),
        Some(_) => decimal_argument(value, name),
        None => Err(tool(format!("{name} is missing"))),
    }
}

fn recovered_page(after: Option<u64>, value: &Value) -> Result<ContextSourcePage, DeveloperLoopError> {
    let values = value.get("sources").and_then(Value::as_array)
        .filter(|values| values.len() <= crate::MAX_CONTEXT_SOURCE_PAGE)
        .ok_or_else(|| tool("invalid recovered request-source page"))?;
    let mut sources = Vec::with_capacity(values.len());
    for value in values {
        let kind = match value.get("kind").and_then(Value::as_str) {
            Some("user_request") => ContextSourceKind::UserRequest,
            Some("assistant_history") => ContextSourceKind::AssistantHistory,
            _ => return Err(tool("recovered request source has no exact semantic role")),
        };
        let label = value.get("label").and_then(Value::as_str)
            .ok_or_else(|| tool("recovered request source has no label"))?;
        let mut source = crate::ContextSource::new(
            required_cursor(value, "source")?,
            label.to_owned(),
            bound_digest(value, "sha256")?,
            required_cursor(value, "bytes")?,
        ).map_err(tool)?.with_kind(kind);
        match value.get("requiresRead").and_then(Value::as_bool) {
            Some(true) => {}
            Some(false) if kind == ContextSourceKind::UserRequest => {
                source = source.with_prompt_body();
            }
            _ => return Err(tool("recovered request source has invalid prompt credit")),
        }
        sources.push(source);
    }
    ContextSourcePage::new(after, sources, nullable_cursor(value, "next")?).map_err(tool)
}

impl WorkspaceDeveloperTools {
    pub(in crate::developer_tools) fn restore_request_source_evidence(&mut self) {
        if !self.grounding.is_for_workspace(&self.root) {
            return;
        }
        self.request_sources.merge(&self.grounding.request_sources);
        let Some(view) = &self.protection_view else { return };
        if self.request_sources.catalog_authority != Some(view.request_source_binding()) {
            return;
        }
        for body in self.request_sources.bodies.values()
            .filter(|body| body.descriptor.kind() == ContextSourceKind::UserRequest)
        {
            self.references.merge(&body.references);
        }
    }

    pub(super) fn request_sources(
        &mut self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let after = decimal_argument(arguments, "after")?;
        let mut progress = self.request_sources.clone();
        let (authority, catalog, page) = {
            let view = self
                .protection_view
                .as_ref()
                .ok_or_else(|| tool("this run has no live request-source authority"))?;
            let authority = view.request_source_binding();
            let catalog = view.request_source_catalog_binding();
            progress.expect_page(after, authority, catalog)?;
            let page = view.request_sources(after).map_err(tool)?;
            if view.request_source_binding() != authority
                || view.request_source_catalog_binding() != catalog
            {
                return Err(tool(
                    "the admitted request-source catalog changed during enumeration",
                ));
            }
            (authority, catalog, page)
        };
        if progress.catalog_binding != Some(catalog)
            || progress.catalog_authority != Some(authority)
        {
            return Err(tool("the admitted request-source catalog is not bound"));
        }
        progress.observe_page(after, &page)?;
        self.request_sources = progress;
        self.grounding.request_sources.clone_from(&self.request_sources);
        let mut value = page_value(page);
        if let Some(sources) = value.get_mut("sources").and_then(Value::as_array_mut) {
            for source in sources {
                if let Ok(ordinal) = required_cursor(source, "source")
                    && let Some(progress) = self.request_sources.bodies.get(&ordinal)
                    && let Some(source) = source.as_object_mut()
                {
                    source.insert("verifiedThrough".to_owned(), Value::String(progress.frontier.to_string()));
                    source.insert("readComplete".to_owned(), Value::Bool(progress.complete));
                }
            }
        }
        Ok(bound_value(value, authority, catalog))
    }

    pub(super) fn request_source_read(
        &mut self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let source = required_cursor(arguments, "source")?;
        let offset = required_cursor(arguments, "offset")?;
        let (authority, catalog, slice) = {
            let view = self
                .protection_view
                .as_ref()
                .ok_or_else(|| tool("this run has no live request-source authority"))?;
            let authority = view.request_source_binding();
            let catalog = view.request_source_catalog_binding();
            if self.request_sources.catalog_binding != Some(catalog)
                || self.request_sources.catalog_authority != Some(authority)
            {
                return Err(tool(
                    "the admitted request-source catalog changed; restart request-source enumeration",
                ));
            }
            self.request_sources.expect_slice(source, offset)?;
            let slice = view.read_request_source(source, offset).map_err(tool)?;
            if view.request_source_binding() != authority
                || view.request_source_catalog_binding() != catalog
            {
                return Err(tool("the admitted request-source catalog changed during a read"));
            }
            (authority, catalog, slice)
        };
        if self.request_sources.catalog_binding != Some(catalog)
            || self.request_sources.catalog_authority != Some(authority)
        {
            return Err(tool("the admitted request-source catalog is not bound"));
        }
        validate_slice(&slice, source, offset, "request source")?;
        if let Some(authority) = self.request_sources.observe_slice(&slice, &self.root)? {
            self.observe_request_authority(&authority);
        }
        self.grounding.request_sources.clone_from(&self.request_sources);
        Ok(bound_value(slice_value(slice), authority, catalog))
    }

    pub(super) fn context_sources(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let after = decimal_argument(arguments, "after")?;
        let view = self
            .protection_view
            .as_ref()
            .ok_or_else(|| tool("this run has no live context-source authority"))?;
        Ok(page_value(view.context_sources(after).map_err(tool)?))
    }

    pub(super) fn context_source_read(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let source = required_cursor(arguments, "source")?;
        let offset = required_cursor(arguments, "offset")?;
        let view = self
            .protection_view
            .as_ref()
            .ok_or_else(|| tool("this run has no live context-source authority"))?;
        let slice = view.read_context_source(source, offset).map_err(tool)?;
        validate_slice(&slice, source, offset, "context source")?;
        Ok(slice_value(slice))
    }
}

fn page_value(page: ContextSourcePage) -> Value {
    let sources = page
        .sources()
        .iter()
        .map(|source| {
            object(vec![
                ("source", Value::String(source.ordinal().to_string())),
                ("kind", Value::String(source.kind().as_str().to_owned())),
                ("label", Value::String(source.label().to_owned())),
                ("sha256", Value::String(hex_bytes(source.digest()))),
                ("bytes", Value::String(source.bytes().to_string())),
                ("requiresRead", Value::Bool(source.requires_read())),
            ])
        })
        .collect();
    object(vec![
        ("sources", Value::Array(sources)),
        (
            "next",
            page.next().map_or(Value::Null, |next| Value::String(next.to_string())),
        ),
    ])
}

fn validate_slice(
    slice: &ContextSourceSlice,
    source: u64,
    offset: u64,
    kind: &str,
) -> Result<(), DeveloperLoopError> {
    if slice.source() != source || slice.offset() != offset {
        return Err(tool(format!("host returned a {kind} slice for another cursor")));
    }
    Ok(())
}

fn slice_value(slice: ContextSourceSlice) -> Value {
    object(vec![
        ("source", Value::String(slice.source().to_string())),
        ("offset", Value::String(slice.offset().to_string())),
        ("text", Value::String(slice.text().to_owned())),
        (
            "next",
            slice.next().map_or(Value::Null, |next| Value::String(next.to_string())),
        ),
        ("complete", Value::Bool(slice.next().is_none())),
    ])
}

fn utf8_suffix(text: &str, maximum: usize) -> String {
    let mut start = text.len().saturating_sub(maximum);
    while !text.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    text[start..].to_owned()
}

fn required_cursor(arguments: &Value, name: &str) -> Result<u64, DeveloperLoopError> {
    decimal_argument(arguments, name)?.ok_or_else(|| tool(format!("{name} is required")))
}

fn decimal_argument(
    arguments: &Value,
    name: &str,
) -> Result<Option<u64>, DeveloperLoopError> {
    arguments
        .get(name)
        .map(|value| {
            let text = value
                .as_str()
                .ok_or_else(|| tool(format!("{name} must be a decimal string")))?;
            let parsed = text
                .parse::<u64>()
                .map_err(|_| tool(format!("{name} is not a canonical u64 decimal")))?;
            if text != parsed.to_string() {
                return Err(tool(format!("{name} is not a canonical u64 decimal")));
            }
            Ok(parsed)
        })
        .transpose()
}

fn hex_bytes(bytes: &[u8]) -> String {
    use core::fmt::Write as _;
    let mut value = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    value
}
