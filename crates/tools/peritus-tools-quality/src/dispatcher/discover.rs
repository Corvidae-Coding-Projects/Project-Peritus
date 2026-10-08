//! Authorized immutable quality surface discovery.

use core::fmt::Write;

use peritus_tool_protocol::{
    BoundedJson, BoundedText, ImplementationIdentity, JsonLimits, SchemaDigest, ToolResult,
    ToolTiming, Truncation, TruncationMetadata,
};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};
use peritus_types::Sha256Digest;
use peritus_workspace::ReadOnlyWorkspace;
use sha2::{Digest, Sha256};

use super::adapter_failure;
use crate::{
    CheckCatalog, CheckDefinition, CheckRequirement, CheckSource, DiscoveryDiagnostic,
    OutputParser, discover_descriptor, discovery::inspect, json_value::object,
};

const PAGE_RECORDS: usize = 128;
const CURSOR_MAGIC: &[u8; 8] = b"PQDCv001";
const CURSOR_BYTES: usize = 112;

/// Reusable inspection dispatcher over one immutable C1 workspace snapshot.
pub struct QualityDiscoverDispatcher<'workspace> {
    workspace: &'workspace ReadOnlyWorkspace,
    explicit: Vec<CheckDefinition>,
    descriptor: peritus_tool_protocol::ToolDescriptor,
    catalog: Option<CheckCatalog>,
}

impl<'workspace> QualityDiscoverDispatcher<'workspace> {
    /// Binds an immutable C1 workspace and explicit typed project/B2 definitions.
    ///
    /// # Errors
    /// Returns a typed failure only if canonical descriptor construction fails.
    pub fn new(
        workspace: &'workspace ReadOnlyWorkspace,
        explicit: Vec<CheckDefinition>,
    ) -> Result<Self, crate::QualityError> {
        Ok(Self { workspace, explicit, descriptor: discover_descriptor()?, catalog: None })
    }

    /// Borrows the combined catalog once inspection has completed, even when page publication
    /// needs a corrected cursor or a larger selected output envelope.
    #[must_use]
    pub const fn catalog(&self) -> Option<&CheckCatalog> {
        self.catalog.as_ref()
    }
}

impl ToolDispatcher for QualityDiscoverDispatcher<'_> {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        self.descriptor.implementation_identity()
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        if invocation.prepared().descriptor_digest() != self.descriptor.descriptor_digest()
            || invocation.prepared().descriptor().name().as_str() != "quality.discover"
        {
            return Err(adapter_failure(
                "quality-discover-invocation",
                "authorized invocation differs from the discovery descriptor",
            ));
        }
        let revision = invocation.binding().revision();
        let permit = invocation.binding();
        let snapshot = self.workspace.snapshot();
        let target_matches = self.workspace.target_binding().is_some_and(|target| {
            target.workspace_id() == permit.revision().workspace_id()
                && target.environment_id() == permit.environment_id()
                && target.resource_id() == permit.resource_id()
        });
        if revision != invocation.prepared().call().revision()
            || revision.workspace_id() != snapshot.workspace_id()
            || revision.workspace_generation() != snapshot.generation()
            || revision.workspace_revision() != snapshot.revision()
            || !target_matches
        {
            return Err(adapter_failure(
                "quality-discover-target",
                "authorized permit target differs from the immutable C1 snapshot",
            ));
        }
        let requested_cursor = cursor_argument(invocation.prepared().call().arguments())?;
        let maximum_output_bytes = selected_output_bytes(invocation.prepared());
        let json_limits = invocation.prepared().call().limits().json_limits();
        let observed_at = invocation.observed_at();
        let prepared = invocation.into_prepared();

        if self.catalog.is_none() {
            let catalog = inspect(self.workspace, self.explicit.clone())
                .map_err(|error| adapter_failure("quality-discovery", error.detail()))?;
            self.catalog = Some(catalog);
        }
        let catalog = self
            .catalog
            .as_ref()
            .expect("quality catalog was retained before page rendering");
        let rendered = render_catalog_page(
            catalog,
            snapshot_binding(self.workspace),
            requested_cursor,
            maximum_output_bytes,
            json_limits,
        )?;
        let summary = if rendered.deferred {
            "quality catalog page needs a larger selected output envelope".to_owned()
        } else {
            format!(
                "quality catalog records {}..{} of {}",
                rendered.start,
                rendered.end,
                catalog.checks().len() + catalog.diagnostics().len()
            )
        };
        let (model, model_truncation) =
            selected_text(summary.clone(), prepared.call().limits().model_bytes());
        let (human, human_truncation) =
            selected_text(summary, prepared.call().limits().human_bytes());
        let timing = ToolTiming::new(observed_at, observed_at)
            .map_err(|error| adapter_failure("quality-discovery-timing", &error.to_string()))?;
        let result = ToolResult::success(
            &prepared,
            rendered.structured,
            human,
            model,
            Vec::new(),
            timing,
            TruncationMetadata {
                output: rendered.truncation,
                model: model_truncation,
                human: human_truncation,
            },
            0,
        )
        .map_err(|error| adapter_failure("quality-discovery-result", &error.to_string()))?;
        Ok(ToolStart::Completed(result))
    }
}

struct RenderedCatalogPage {
    structured: BoundedJson,
    start: usize,
    end: usize,
    truncation: Truncation,
    deferred: bool,
}

fn render_catalog_page(
    catalog: &CheckCatalog,
    snapshot: Sha256Digest,
    requested: Option<CatalogCursor>,
    maximum_output_bytes: usize,
    json_limits: JsonLimits,
) -> Result<RenderedCatalogPage, DispatchFailure> {
    let total = catalog.checks().len() + catalog.diagnostics().len();
    let binding = CursorBinding { snapshot, catalog: catalog.digest() };
    let start = requested
        .map(|cursor| usize::try_from(cursor.start))
        .transpose()
        .map_err(|_| {
            adapter_failure(
                "quality-discovery-cursor",
                "discovery cursor offset is not representable on this host",
            )
        })?
        .unwrap_or(0);
    if requested.is_some_and(|cursor| {
        cursor.snapshot != binding.snapshot || cursor.catalog != binding.catalog
    }) || (requested.is_some() && start >= total)
    {
        return Err(adapter_failure(
            "quality-discovery-cursor",
            "discovery cursor is outside or differs from the immutable snapshot catalog",
        ));
    }
    let mut end = start.saturating_add(PAGE_RECORDS).min(total);
    loop {
        let cursor = binding.cursor(start).encode();
        let next = (end < total).then(|| binding.cursor(end).encode());
        let value = catalog_page_value(catalog, start, end, &cursor, next.as_deref());
        let encoded = value.to_string();
        let minimum_output_bytes = encoded.len();
        match BoundedJson::parse(&encoded, json_limits) {
            Ok(structured)
                if structured.canonical_bytes().len() <= maximum_output_bytes =>
            {
                return Ok(RenderedCatalogPage {
                    structured,
                    start,
                    end,
                    truncation: page_truncation(start, end < total),
                    deferred: false,
                });
            }
            rendered if end.saturating_sub(start) > 1 => {
                end = start + ((end - start) / 2).max(1);
                drop(rendered);
            }
            Ok(_) => {
                let deferred = deferred_value(
                    catalog,
                    (!total.eq(&0)).then_some(cursor.as_str()),
                    minimum_output_bytes,
                );
                let structured = BoundedJson::parse(&deferred.to_string(), json_limits)
                    .map_err(|error| {
                        adapter_failure("quality-discovery-result", &error.to_string())
                    })?;
                if structured.canonical_bytes().len() > maximum_output_bytes {
                    return Err(adapter_failure(
                        "quality-discovery-output-envelope",
                        "selected output capacity cannot carry a discovery continuation",
                    ));
                }
                return Ok(RenderedCatalogPage {
                    structured,
                    start,
                    end: start,
                    truncation: Truncation::Indeterminate,
                    deferred: true,
                });
            }
            Err(error) => {
                return Err(adapter_failure(
                    "quality-discovery-record",
                    &format!(
                        "one discovery record exceeds the protocol JSON shape and cannot be paged: {error}"
                    ),
                ));
            }
        }
    }
}

fn catalog_page_value(
    catalog: &CheckCatalog,
    start: usize,
    end: usize,
    cursor: &str,
    next_cursor: Option<&str>,
) -> serde_json::Value {
    let check_total = catalog.checks().len();
    let diagnostic_total = catalog.diagnostics().len();
    let check_start = start.min(check_total);
    let check_end = end.min(check_total);
    let diagnostic_start = start.saturating_sub(check_total).min(diagnostic_total);
    let diagnostic_end = end.saturating_sub(check_total).min(diagnostic_total);
    let checks = catalog.checks()[check_start..check_end]
        .iter()
        .map(|entry| check_value(entry.definition()))
        .collect();
    let diagnostics = catalog.diagnostics()[diagnostic_start..diagnostic_end]
        .iter()
        .map(diagnostic_value)
        .collect();
    let complete_page = next_cursor.is_none();
    object([
        ("catalog_complete", serde_json::Value::Bool(catalog.coverage_complete())),
        ("catalog_digest", serde_json::Value::String(hex(catalog.digest().as_bytes()))),
        ("check_count", unsigned(check_total)),
        ("checks", serde_json::Value::Array(checks)),
        (
            "coverage_complete",
            serde_json::Value::Bool(complete_page && catalog.coverage_complete()),
        ),
        ("cursor", serde_json::Value::String(cursor.to_owned())),
        ("diagnostic_count", unsigned(diagnostic_total)),
        ("diagnostics", serde_json::Value::Array(diagnostics)),
        (
            "next_cursor",
            next_cursor.map_or(serde_json::Value::Null, |value| {
                serde_json::Value::String(value.to_owned())
            }),
        ),
        ("record_count", unsigned(check_total + diagnostic_total)),
        ("record_range_end", unsigned(end)),
        ("record_range_start", unsigned(start)),
        ("returned_check_count", unsigned(check_end - check_start)),
        (
            "returned_diagnostic_count",
            unsigned(diagnostic_end - diagnostic_start),
        ),
        (
            "truncated",
            serde_json::Value::Bool(start > 0 || next_cursor.is_some()),
        ),
    ])
}

fn deferred_value(
    catalog: &CheckCatalog,
    cursor: Option<&str>,
    minimum_output_bytes: usize,
) -> serde_json::Value {
    object([
        ("catalog_digest", serde_json::Value::String(hex(catalog.digest().as_bytes()))),
        ("coverage_complete", serde_json::Value::Bool(false)),
        ("minimum_output_bytes", unsigned(minimum_output_bytes)),
        (
            "next_cursor",
            cursor.map_or(serde_json::Value::Null, |value| {
                serde_json::Value::String(value.to_owned())
            }),
        ),
        ("retry_same_page", serde_json::Value::Bool(cursor.is_some())),
        ("retry_same_request", serde_json::Value::Bool(cursor.is_none())),
        ("truncated", serde_json::Value::Bool(true)),
    ])
}

fn check_value(check: &CheckDefinition) -> serde_json::Value {
    object([
        (
            "arguments",
            serde_json::Value::Array(
                check.arguments().iter().cloned().map(serde_json::Value::String).collect(),
            ),
        ),
        (
            "environment_profile",
            serde_json::Value::String(check.environment_profile().as_str().to_owned()),
        ),
        ("executable", serde_json::Value::String(check.executable().to_owned())),
        (
            "expected_success",
            serde_json::Value::String(expected_success_name(check.expected_success())),
        ),
        ("gate_id", serde_json::Value::String(hex(check.gate_id().as_bytes()))),
        ("gate_name", serde_json::Value::String(check.gate_name().to_owned())),
        (
            "output_bytes",
            check.output_limit().map_or(serde_json::Value::Null, |output| {
                serde_json::Value::String(output.to_string())
            }),
        ),
        ("parser", serde_json::Value::String(parser_name(check.parser()).to_owned())),
        (
            "requirement",
            serde_json::Value::String(requirement_name(check.requirement()).to_owned()),
        ),
        ("source", serde_json::Value::String(source_name(check.source()))),
        (
            "timeout_millis",
            check.timeout_millis().map_or(serde_json::Value::Null, |timeout| {
                serde_json::Value::String(timeout.to_string())
            }),
        ),
        (
            "working_directory",
            check.working_directory().map_or(serde_json::Value::Null, |path| {
                serde_json::Value::String(path.to_string())
            }),
        ),
    ])
}

fn diagnostic_value(diagnostic: &DiscoveryDiagnostic) -> serde_json::Value {
    object([
        (
            "cause",
            serde_json::Value::String(diagnostic.cause().as_str().to_owned()),
        ),
        (
            "coverage",
            serde_json::Value::String(diagnostic.coverage().as_str().to_owned()),
        ),
        ("detail", serde_json::Value::String(diagnostic.detail().to_owned())),
        ("source", serde_json::Value::String(diagnostic.source().to_owned())),
    ])
}

fn cursor_argument(arguments: &BoundedJson) -> Result<Option<CatalogCursor>, DispatchFailure> {
    let Some(value) = arguments.property("cursor") else { return Ok(None) };
    let value = value.as_str().ok_or_else(|| {
        adapter_failure("quality-discovery-cursor", "discovery cursor is not a string")
    })?;
    CatalogCursor::decode(value).map(Some).map_err(|()| {
        adapter_failure("quality-discovery-cursor", "discovery cursor is malformed")
    })
}

fn selected_output_bytes(prepared: &peritus_tool_protocol::PreparedToolCall) -> usize {
    let json_bytes = prepared.call().limits().json_limits().max_bytes();
    prepared.call().limits().output_limit().map_or(json_bytes, |value| {
        usize::try_from(value).unwrap_or(usize::MAX).min(json_bytes)
    })
}

fn selected_text(value: String, maximum: u32) -> (BoundedText, Truncation) {
    let mut value = value.replace('\0', "\\0");
    let complete = value.len() <= maximum as usize;
    crate::error::truncate_utf8(&mut value, maximum as usize);
    if value.is_empty() {
        value.push('.');
    }
    (
        BoundedText::new(value).expect("selected nonempty quality text is protocol bounded"),
        if complete { Truncation::Complete } else { Truncation::TailDropped },
    )
}

const fn page_truncation(has_prior: usize, has_next: bool) -> Truncation {
    match (has_prior > 0, has_next) {
        (false, false) => Truncation::Complete,
        (false, true) => Truncation::TailDropped,
        (true, false) => Truncation::HeadDropped,
        (true, true) => Truncation::Windowed,
    }
}

#[derive(Clone, Copy)]
struct CursorBinding {
    snapshot: Sha256Digest,
    catalog: Sha256Digest,
}

impl CursorBinding {
    fn cursor(self, start: usize) -> CatalogCursor {
        CatalogCursor {
            snapshot: self.snapshot,
            catalog: self.catalog,
            start: u64::try_from(start).expect("bounded catalog offset fits u64"),
        }
    }
}

#[derive(Clone, Copy)]
struct CatalogCursor {
    snapshot: Sha256Digest,
    catalog: Sha256Digest,
    start: u64,
}

impl CatalogCursor {
    fn encode(self) -> String {
        let mut bytes = CURSOR_MAGIC.to_vec();
        bytes.extend_from_slice(self.snapshot.as_bytes());
        bytes.extend_from_slice(self.catalog.as_bytes());
        bytes.extend_from_slice(&self.start.to_be_bytes());
        let seal: [u8; 32] = Sha256::digest(&bytes).into();
        bytes.extend_from_slice(&seal);
        hex(&bytes)
    }

    fn decode(value: &str) -> Result<Self, ()> {
        let bytes = decode_hex(value)?;
        if bytes.len() != CURSOR_BYTES || !bytes.starts_with(CURSOR_MAGIC) {
            return Err(());
        }
        let payload = &bytes[..CURSOR_BYTES - 32];
        let seal: [u8; 32] = Sha256::digest(payload).into();
        if seal.as_slice() != &bytes[CURSOR_BYTES - 32..] {
            return Err(());
        }
        Ok(Self {
            snapshot: Sha256Digest::new(array(&bytes[8..40])?),
            catalog: Sha256Digest::new(array(&bytes[40..72])?),
            start: u64::from_be_bytes(array(&bytes[72..80])?),
        })
    }
}

fn snapshot_binding(workspace: &ReadOnlyWorkspace) -> Sha256Digest {
    let snapshot = workspace.snapshot();
    let mut hash = Sha256::new();
    hash.update(b"peritus-quality-discovery-snapshot-v1\0");
    hash.update(snapshot.workspace_id().as_bytes());
    hash.update(snapshot.generation().get().to_be_bytes());
    hash.update(snapshot.revision().get().to_be_bytes());
    let commit = snapshot.commit().object_id();
    hash.update(commit.format().as_str().as_bytes());
    hash.update((commit.as_bytes().len() as u64).to_be_bytes());
    hash.update(commit.as_bytes());
    let tree = snapshot.tree().object_id();
    hash.update(tree.format().as_str().as_bytes());
    hash.update((tree.as_bytes().len() as u64).to_be_bytes());
    hash.update(tree.as_bytes());
    Sha256Digest::new(hash.finalize().into())
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ()> {
    if value.len() != CURSOR_BYTES * 2
        || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(nibble(pair[0])? * 16 + nibble(pair[1])?))
        .collect()
}

const fn nibble(value: u8) -> Result<u8, ()> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(()),
    }
}

fn array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ()> {
    bytes.try_into().map_err(|_| ())
}

fn unsigned(value: usize) -> serde_json::Value {
    serde_json::Value::from(u64::try_from(value).expect("bounded catalog count fits u64"))
}

const fn requirement_name(value: CheckRequirement) -> &'static str {
    match value {
        CheckRequirement::Required => "required",
        CheckRequirement::Optional => "optional",
        CheckRequirement::Discovered => "discovered",
    }
}

fn source_name(value: &CheckSource) -> String {
    match value {
        CheckSource::Explicit(label) => format!("explicit:{label}"),
        CheckSource::CargoManifest => "cargo-manifest".to_owned(),
        CheckSource::JustfileRecipe(recipe) => format!("justfile:{recipe}"),
    }
}

const fn parser_name(value: OutputParser) -> &'static str {
    match value {
        OutputParser::None => "none",
        OutputParser::Utf8 { .. } => "utf8",
        OutputParser::Json { .. } => "json",
        OutputParser::JsonSuccess { .. } => "json-success",
    }
}

fn expected_success_name(value: crate::ExpectedSuccess) -> String {
    match value {
        crate::ExpectedSuccess::ExitCode(code) => format!("exit-code:{code}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut value, byte| {
        write!(value, "{byte:02x}").expect("writing to a string cannot fail");
        value
    })
}
