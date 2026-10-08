//! Bounded model, human, and structured filesystem renderings.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_tool_protocol::{BoundedJson, BoundedText, JsonLimits};

use crate::{
    DiscoverExclusion, DiscoverObservation, FileContent, FileObservation, FsToolError,
    FsToolErrorKind, FsToolOperation, MetadataObservation, RecoveryClass, SearchObservation,
};

const MAX_RENDER_ITEMS: usize = 500;
const TEXT_LIMIT: usize = 16 * 1024;

/// Independently bounded structured, model, and human renderings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedOutput {
    structured: BoundedJson,
    model: BoundedText,
    human: BoundedText,
    truncated: bool,
}

impl RenderedOutput {
    /// Returns the exact canonical JSON byte length of the structured result.
    #[must_use]
    pub fn encoded_bytes(&self) -> usize {
        self.structured.canonical_bytes().len()
    }

    /// Renders a compact continuation when one physical record cannot fit the selected envelope.
    pub fn deferred(
        operation: &'static str,
        cursor: Option<&str>,
        minimum_output_bytes: usize,
    ) -> Result<Self, FsToolError> {
        let structured = object(vec![
            ("coverage_complete", Ok(BoundedJson::boolean(false))),
            (
                "minimum_output_bytes",
                unsigned_usize(minimum_output_bytes),
            ),
            ("next_cursor", optional_string(cursor)),
            ("operation", string(operation.to_owned())),
            ("retry_same_request", Ok(BoundedJson::boolean(cursor.is_none()))),
            ("truncated", Ok(BoundedJson::boolean(true))),
        ])?;
        let text = format!(
            "{operation} requires at least {minimum_output_bytes} output bytes for its next physical record"
        );
        finish(structured, text.clone(), text, true)
    }

    /// Renders exact authorized C1 patch outcome evidence.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn mutation(value: &peritus_workspace::MutationOutcome) -> Result<Self, FsToolError> {
        Self::mutation_reference(value.reference())
    }

    /// Renders restart-visible authorized C1 patch outcome evidence.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn mutation_reference(
        value: peritus_workspace::MutationOutcomeReference,
    ) -> Result<Self, FsToolError> {
        let operation = value.operation();
        let structured = object(vec![
            ("action_id", string(identifier_hex(value.action_id().as_bytes()))),
            ("generation", Ok(BoundedJson::unsigned(value.generation().get()))),
            (
                "installed_manifest_digest",
                string(digest_hex(value.installed_manifest_digest())),
            ),
            ("mutation_operation_digest", string(digest_hex(operation.digest()))),
            (
                "mutation_operation_reference",
                string(STANDARD.encode(operation.canonical_bytes())),
            ),
            ("mutation_outcome_digest", string(digest_hex(value.digest()))),
            (
                "mutation_outcome_reference",
                string(STANDARD.encode(value.canonical_bytes())),
            ),
            ("patch_identity", string(value.patch_identity().to_string())),
            ("resource_id", string(identifier_hex(value.resource_id().as_bytes()))),
            ("revision", Ok(BoundedJson::unsigned(value.revision().get()))),
            ("workspace_id", string(identifier_hex(value.workspace_id().as_bytes()))),
        ])?;
        let text = format!("Applied authorized patch {}.", value.patch_identity());
        finish(structured, text.clone(), text, false)
    }

    /// Renders one metadata observation.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn metadata(value: &MetadataObservation) -> Result<Self, FsToolError> {
        let structured = metadata_json(value)?;
        let text = format!(
            "{}: {} bytes, {}, executable={}",
            value.path().as_str().escape_debug(),
            value.size(),
            kind_name(value),
            value.executable()
        );
        finish(structured, text.clone(), text, false)
    }

    /// Renders bounded discovery entries and truthful truncation state.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn discover(value: &DiscoverObservation) -> Result<Self, FsToolError> {
        let retained = value.entries().len().min(MAX_RENDER_ITEMS);
        let excluded_retained = value.exclusions().len().min(MAX_RENDER_ITEMS - retained);
        let omitted_retained = value
            .omissions()
            .len()
            .min(MAX_RENDER_ITEMS - retained - excluded_retained);
        let truncated = value.next_cursor().is_some()
            || retained < value.entries().len()
            || excluded_retained < value.exclusions().len()
            || omitted_retained < value.omissions().len();
        let entries = value.entries()[..retained]
            .iter()
            .map(|entry| {
                object(vec![
                    ("depth", Ok(BoundedJson::unsigned(u64::from(entry.depth())))),
                    ("metadata", metadata_json(entry.metadata())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields = vec![
            ("cursor", string(value.cursor().to_owned())),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("entry_count", Ok(BoundedJson::unsigned(value.entry_count()))),
            ("next_cursor", optional_string(value.next_cursor())),
            ("observed_count", Ok(BoundedJson::unsigned(value.entry_count()))),
            ("omitted_count", Ok(BoundedJson::unsigned(value.omission_count()))),
            ("record_count", Ok(BoundedJson::unsigned(value.record_count()))),
            (
                "root",
                value
                    .root()
                    .map_or_else(|| Ok(BoundedJson::null()), |path| string(path.to_string())),
            ),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ];
        append_exclusions(&mut fields, value.exclusions(), excluded_retained)?;
        fields.push((
            "total_excluded_count",
            Ok(BoundedJson::unsigned(value.exclusion_count())),
        ));
        append_traversal_omissions(&mut fields, value.omissions(), omitted_retained)?;
        let structured = object(fields)?;
        let text = format!(
            "Discovered {} workspace entries{}.",
            value.entry_count(),
            if truncated { " (rendered window truncated)" } else { "" }
        );
        let text = with_exclusion_summary(text, value.exclusions());
        finish(structured, text.clone(), text, truncated)
    }

    /// Renders an exact bounded text or base64 file observation.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn file(value: &FileObservation) -> Result<Self, FsToolError> {
        let (encoding, content) = match value.content() {
            FileContent::Utf8(text) => ("utf8", text.clone()),
            FileContent::Base64(text) => ("base64", text.clone()),
        };
        let structured = object(vec![
            ("content", string(content)),
            ("content_digest", string(digest_hex(value.content_digest()))),
            ("cursor", string(value.cursor().to_owned())),
            ("encoding", string(encoding.to_owned())),
            ("metadata", metadata_json(value.metadata())),
            ("next_cursor", optional_string(value.next_cursor())),
            ("range_end", Ok(BoundedJson::unsigned(value.range().1))),
            ("range_start", Ok(BoundedJson::unsigned(value.range().0))),
            ("source_bytes", Ok(BoundedJson::unsigned(value.source_bytes()))),
            ("source_digest", string(digest_hex(value.source_digest()))),
        ])?;
        let text = format!(
            "Read {} exact bytes from {} as {encoding}.",
            value.range().1 - value.range().0,
            value.metadata().path().as_str().escape_debug()
        );
        finish(structured, text.clone(), text, value.next_cursor().is_some())
    }

    /// Renders a bounded window of structured literal matches.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn search(value: &SearchObservation) -> Result<Self, FsToolError> {
        let retained = value.matches().len().min(MAX_RENDER_ITEMS);
        let excluded_retained = value.exclusions().len().min(MAX_RENDER_ITEMS - retained);
        let traversal_retained = value
            .traversal_omissions()
            .len()
            .min(MAX_RENDER_ITEMS - retained - excluded_retained);
        let omitted_retained = value.omissions().len().min(
            MAX_RENDER_ITEMS - retained - excluded_retained - traversal_retained,
        );
        let truncated = value.next_cursor().is_some()
            || retained < value.matches().len()
            || excluded_retained < value.exclusions().len()
            || traversal_retained < value.traversal_omissions().len()
            || omitted_retained < value.omissions().len();
        let matches = value.matches()[..retained]
            .iter()
            .map(|value| {
                object(vec![
                    (
                        "column_bytes",
                        Ok(BoundedJson::unsigned(u64::from(value.column_bytes()))),
                    ),
                    ("line", Ok(BoundedJson::unsigned(value.line()))),
                    ("path", string(value.path().to_string())),
                ("preview", string(value.preview().to_owned())),
                (
                    "preview_start_column_bytes",
                    Ok(BoundedJson::unsigned(value.preview_start_column_bytes())),
                ),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields = vec![
            ("case_semantics", string(value.case_semantics().to_owned())),
            ("coverage_complete", Ok(BoundedJson::boolean(value.next_cursor().is_none()))),
            ("cursor", string(value.cursor().to_owned())),
            ("digest", string(digest_hex(value.digest()))),
            ("match_count", Ok(BoundedJson::unsigned(value.match_count()))),
            ("matches", array(matches)),
            ("next_cursor", optional_string(value.next_cursor())),
            ("omitted_count", Ok(BoundedJson::unsigned(value.omission_count()))),
            (
                "returned_match_count",
                unsigned_usize(value.matches().len()),
            ),
            ("scanned_bytes", Ok(BoundedJson::unsigned(value.scanned_bytes()))),
            ("scanned_files", Ok(BoundedJson::unsigned(u64::from(value.scanned_files())))),
            (
                "traversal_omitted_count",
                Ok(BoundedJson::unsigned(value.traversal_omission_count())),
            ),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ];
        append_exclusions(&mut fields, value.exclusions(), excluded_retained)?;
        fields.push((
            "total_excluded_count",
            Ok(BoundedJson::unsigned(value.exclusion_count())),
        ));
        append_traversal_omissions(
            &mut fields,
            value.traversal_omissions(),
            traversal_retained,
        )?;
        append_search_omissions(&mut fields, value.omissions(), omitted_retained)?;
        let structured = object(fields)?;
        let text = format!(
            "Found {} literal matches across {} UTF-8 files ({} bytes scanned){}.",
            value.match_count(),
            value.scanned_files(),
            value.scanned_bytes(),
            if truncated { " Rendered match window is truncated" } else { "" }
        );
        let text = with_exclusion_summary(text, value.exclusions());
        finish(structured, text.clone(), text, truncated)
    }

    /// Returns canonical bounded structured output.
    #[must_use]
    pub const fn structured(&self) -> &BoundedJson {
        &self.structured
    }
    /// Returns bounded model-facing text.
    #[must_use]
    pub const fn model(&self) -> &BoundedText {
        &self.model
    }
    /// Returns bounded human-facing text.
    #[must_use]
    pub const fn human(&self) -> &BoundedText {
        &self.human
    }
    /// Returns whether only a window of structured entries or matches was rendered.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

fn append_exclusions(
    fields: &mut Vec<(&'static str, Result<BoundedJson, FsToolError>)>,
    exclusions: &[DiscoverExclusion],
    retained: usize,
) -> Result<(), FsToolError> {
    if exclusions.is_empty() {
        return Ok(());
    }
    let values = exclusions[..retained]
        .iter()
        .map(|value| {
            object(vec![
                ("depth", Ok(BoundedJson::unsigned(u64::from(value.depth())))),
                (
                    "directory",
                    value
                        .directory()
                        .map_or_else(|| Ok(BoundedJson::null()), |path| string(path.to_string())),
                ),
                ("display_name", string(value.name().display_name())),
                (
                    "native_encoding",
                    string(
                        match value.name().encoding() {
                            peritus_workspace::NativeNameEncoding::UnixBytes => "unix-bytes",
                            peritus_workspace::NativeNameEncoding::WindowsWide => {
                                "windows-utf16-be"
                            }
                            peritus_workspace::NativeNameEncoding::Utf8 => "utf8",
                        }
                        .to_owned(),
                    ),
                ),
                ("native_units_base64", string(STANDARD.encode(value.name().encoded_bytes()))),
                ("reason", string(value.reason().as_str().to_owned())),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    fields.push((
        "excluded_count",
        unsigned_usize(exclusions.len()),
    ));
    fields.push(("exclusions", array(values)));
    Ok(())
}

fn append_traversal_omissions(
    fields: &mut Vec<(&'static str, Result<BoundedJson, FsToolError>)>,
    omissions: &[crate::exclusion::TraversalOmission],
    retained: usize,
) -> Result<(), FsToolError> {
    let values = omissions[..retained]
        .iter()
        .map(|value| {
            object(vec![
                ("depth", Ok(BoundedJson::unsigned(u64::from(value.depth())))),
                ("path", string(value.path().to_string())),
                ("reason", string("maximum_depth".to_owned())),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    fields.push(("traversal_omissions", array(values)));
    Ok(())
}

fn append_search_omissions(
    fields: &mut Vec<(&'static str, Result<BoundedJson, FsToolError>)>,
    omissions: &[crate::SearchOmission],
    retained: usize,
) -> Result<(), FsToolError> {
    let values = omissions[..retained]
        .iter()
        .map(|value| {
            object(vec![
                ("path", string(value.path().to_string())),
                ("reason", string(value.reason().as_str().to_owned())),
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    fields.push(("omissions", array(values)));
    Ok(())
}

fn with_exclusion_summary(mut text: String, exclusions: &[DiscoverExclusion]) -> String {
    use std::fmt::Write as _;
    if !exclusions.is_empty() {
        write!(&mut text, " {} native children have explicit exclusions.", exclusions.len())
            .expect("string formatting");
    }
    text
}

fn metadata_json(value: &MetadataObservation) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("executable", Ok(BoundedJson::boolean(value.executable()))),
        ("kind", string(kind_name(value).to_owned())),
        ("path", string(value.path().to_string())),
        ("size", Ok(BoundedJson::unsigned(value.size()))),
    ])
}

const fn kind_name(value: &MetadataObservation) -> &'static str {
    match value.kind() {
        peritus_workspace::WorkspaceEntryKind::File => "file",
        peritus_workspace::WorkspaceEntryKind::Directory => "directory",
    }
}

fn finish(
    structured: BoundedJson,
    model: String,
    human: String,
    truncated: bool,
) -> Result<RenderedOutput, FsToolError> {
    let model = BoundedText::new(cap_text(model)).map_err(|_| protocol_error())?;
    let human = BoundedText::new(cap_text(human)).map_err(|_| protocol_error())?;
    Ok(RenderedOutput { structured, model, human, truncated })
}

fn object(
    members: Vec<(&str, Result<BoundedJson, FsToolError>)>,
) -> Result<BoundedJson, FsToolError> {
    let members = members
        .into_iter()
        .map(|(name, value)| value.map(|value| (name.to_owned(), value)))
        .collect::<Result<Vec<_>, _>>()?;
    BoundedJson::object(members, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn array(values: Vec<BoundedJson>) -> Result<BoundedJson, FsToolError> {
    BoundedJson::array(values, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn string(value: String) -> Result<BoundedJson, FsToolError> {
    BoundedJson::string(value, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn optional_string(value: Option<&str>) -> Result<BoundedJson, FsToolError> {
    value.map_or_else(|| Ok(BoundedJson::null()), |value| string(value.to_owned()))
}

fn integer(value: i64) -> BoundedJson {
    BoundedJson::integer(value)
}

fn unsigned_usize(value: usize) -> Result<BoundedJson, FsToolError> {
    u64::try_from(value)
        .map(BoundedJson::unsigned)
        .map_err(|_| protocol_error())
}

fn digest_hex(value: peritus_types::Sha256Digest) -> String {
    identifier_hex(value.as_bytes())
}

fn identifier_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn cap_text(mut value: String) -> String {
    if value.len() <= TEXT_LIMIT {
        return value;
    }
    let mut end = TEXT_LIMIT;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

const fn protocol_error() -> FsToolError {
    FsToolError::new(
        FsToolErrorKind::Protocol,
        FsToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "filesystem tool output exceeded the bounded protocol",
    )
}
