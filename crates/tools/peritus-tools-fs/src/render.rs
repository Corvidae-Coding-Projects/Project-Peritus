//! Bounded model, human, and structured filesystem renderings.

use peritus_tool_protocol::{BoundedJson, BoundedText, JsonLimits};

use crate::{
    DiscoverObservation, FileContent, FileObservation, FsToolError, FsToolErrorKind,
    FsToolOperation, MetadataObservation, RecoveryClass, SearchObservation,
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
    /// Renders exact authorized C1 patch outcome evidence.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn mutation(value: &peritus_workspace::MutationOutcome) -> Result<Self, FsToolError> {
        let structured = object(vec![
            ("action_id", string(identifier_hex(value.action_id().as_bytes()))),
            ("generation", Ok(integer(u64_integer(value.generation().get())))),
            ("patch_identity", string(value.patch_identity().to_string())),
            ("revision", Ok(integer(u64_integer(value.revision().get())))),
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
            value.path(),
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
        let truncated = retained < value.entries().len();
        let entries = value.entries()[..retained]
            .iter()
            .map(|entry| {
                object(vec![
                    ("depth", Ok(integer(i64::from(entry.depth())))),
                    ("metadata", metadata_json(entry.metadata())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("observed_count", Ok(integer(usize_integer(value.entries().len())))),
            (
                "root",
                value
                    .root()
                    .map_or_else(|| Ok(BoundedJson::null()), |path| string(path.to_string())),
            ),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ])?;
        let text = format!(
            "Discovered {} workspace entries{}.",
            value.entries().len(),
            if truncated { " (rendered window truncated)" } else { "" }
        );
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
            ("encoding", string(encoding.to_owned())),
            ("metadata", metadata_json(value.metadata())),
        ])?;
        let text = format!(
            "Read {} exact bytes from {} as {encoding}.",
            value.metadata().size(),
            value.metadata().path()
        );
        finish(structured, text.clone(), text, false)
    }

    /// Renders a bounded window of structured literal matches.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn search(value: &SearchObservation) -> Result<Self, FsToolError> {
        let retained = value.matches().len().min(MAX_RENDER_ITEMS);
        let truncated = retained < value.matches().len();
        let matches = value.matches()[..retained]
            .iter()
            .map(|value| {
                object(vec![
                    ("column_bytes", Ok(integer(i64::from(value.column_bytes())))),
                    ("line", Ok(integer(u64_integer(value.line())))),
                    ("path", string(value.path().to_string())),
                    ("preview", string(value.preview().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("digest", string(digest_hex(value.digest()))),
            ("match_count", Ok(integer(usize_integer(value.matches().len())))),
            ("matches", array(matches)),
            ("scanned_bytes", Ok(integer(u64_integer(value.scanned_bytes())))),
            ("scanned_files", Ok(integer(i64::from(value.scanned_files())))),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ])?;
        let text = format!(
            "Found {} literal matches across {} UTF-8 files ({} bytes scanned){}.",
            value.matches().len(),
            value.scanned_files(),
            value.scanned_bytes(),
            if truncated { " Rendered match window is truncated" } else { "" }
        );
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

fn metadata_json(value: &MetadataObservation) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("executable", Ok(BoundedJson::boolean(value.executable()))),
        ("kind", string(kind_name(value).to_owned())),
        ("path", string(value.path().to_string())),
        ("size", Ok(integer(u64_integer(value.size())))),
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

fn integer(value: i64) -> BoundedJson {
    BoundedJson::integer(value)
}

fn u64_integer(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn usize_integer(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
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
