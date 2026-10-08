//! Bounded model, human, and structured filesystem renderings.

mod fragments;
mod json;
mod pages;
#[cfg(test)]
mod tests;

use json::{array, object, string};

use peritus_tool_protocol::{BoundedJson, BoundedText};

use crate::{
    DiscoverObservation, FileContent, FileObservation, FsToolError, FsToolErrorKind,
    FsToolOperation, MetadataObservation, OmissionReason, RecoveryClass, ScopeOmission,
    SearchObservation,
};

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
            ("generation", u64_integer(value.generation().get())),
            ("patch_identity", string(value.patch_identity().to_string())),
            ("revision", u64_integer(value.revision().get())),
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
            "Observed {} bytes, {}, executable={}",
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
    pub fn discover_page(
        value: &DiscoverObservation,
        offset: u64,
        maximum_items: u32,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        let page_offset = offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?;
        let start =
            usize::try_from(page_offset).map_err(|_| protocol_error())?.min(value.entries().len());
        let end = start.saturating_add(maximum_items as usize).min(value.entries().len());
        let global_start = value
            .page_start()
            .checked_add(u64::try_from(start).map_err(|_| protocol_error())?)
            .ok_or_else(protocol_error)?;
        let mut entries = Vec::new();
        let mut structured = discover_json_with_page(
            value,
            &entries,
            global_start,
            global_start < value.observed_count(),
        )?;
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        for entry in value.entries().get(start..end).unwrap_or_default() {
            let item = object(vec![
                ("depth", Ok(integer(i64::from(entry.depth())))),
                ("metadata", metadata_json(entry.metadata())),
                (
                    "omission",
                    entry.omission_reason().map_or_else(
                        || Ok(BoundedJson::null()),
                        |reason| string(omission_name(reason).to_owned()),
                    ),
                ),
            ])?;
            entries.push(item);
            let retained_end = global_start
                .checked_add(u64::try_from(entries.len()).map_err(|_| protocol_error())?)
                .ok_or_else(protocol_error)?;
            let candidate = discover_json_with_page(
                value,
                &entries,
                retained_end,
                retained_end < value.observed_count(),
            );
            match candidate {
                Ok(candidate) if candidate.canonical_bytes().len() as u64 <= output_bytes => {}
                _ => {
                    entries.pop();
                    break;
                }
            }
        }
        let retained_end = global_start
            .checked_add(u64::try_from(entries.len()).map_err(|_| protocol_error())?)
            .ok_or_else(protocol_error)?;
        let more = retained_end < value.observed_count();
        if more && entries.is_empty() {
            return Err(protocol_error());
        }
        structured = discover_json_with_page(value, &entries, retained_end, more)?;
        let text = format!(
            "Discovered entries {} through {} of {}{}.",
            if entries.is_empty() { 0 } else { global_start.saturating_add(1) },
            retained_end,
            value.observed_count(),
            if more { " (more entries continue)" } else { "" }
        );
        finish(structured, text.clone(), text, more)
    }

    /// Renders an exact bounded text or base64 file observation.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn file(value: &FileObservation, output_bytes: u64) -> Result<Self, FsToolError> {
        let (encoding, content) = match value.content() {
            FileContent::Utf8(text) => ("utf8", text.clone()),
            FileContent::Base64(text) => ("base64", text.clone()),
        };
        let structured = object(vec![
            ("content", string(content)),
            ("content_digest", string(digest_hex(value.content_digest()))),
            ("encoding", string(encoding.to_owned())),
            ("metadata", metadata_json(value.metadata())),
            ("range_end", u64_integer(value.range().1)),
            ("range_start", u64_integer(value.range().0)),
            ("source_digest", string(digest_hex(value.source_digest()))),
            (
                "continuation_offset",
                value.continuation_offset().map_or_else(|| Ok(BoundedJson::null()), u64_integer),
            ),
        ])?;
        let text = format!(
            "Read bytes {} through {} of {} as {encoding}.",
            value.range().0,
            value.range().1,
            value.metadata().size(),
        );
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        finish(structured, text.clone(), text, value.continuation_offset().is_some())
    }

    /// Renders a bounded window of structured literal matches.
    ///
    /// # Errors
    /// Returns a typed protocol failure if bounded encoding cannot be constructed.
    pub fn search_page(
        value: &SearchObservation,
        offset: u64,
        maximum_items: u32,
        omission_offset: u64,
        output_bytes: u64,
    ) -> Result<Self, FsToolError> {
        let match_start =
            usize::try_from(offset.checked_sub(value.page_start()).ok_or_else(protocol_error)?)
                .map_err(|_| protocol_error())?
                .min(value.matches().len());
        let match_end =
            match_start.saturating_add(maximum_items as usize).min(value.matches().len());
        let omission_start = usize::try_from(
            omission_offset.checked_sub(value.omission_page_start()).ok_or_else(protocol_error)?,
        )
        .map_err(|_| protocol_error())?
        .min(value.omissions().len());
        let omission_end =
            omission_start.saturating_add(maximum_items as usize).min(value.omissions().len());
        let mut matches = Vec::new();
        let mut omissions = Vec::new();
        let mut structured = search_json(
            value,
            &matches,
            &omissions,
            next_match_offset(value, offset, matches.len())?,
            next_omission_offset(value, omission_start, omissions.len(), omission_offset)?,
        )?;
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        for matched in value.matches().get(match_start..match_end).unwrap_or_default() {
            let item = search_match_json(matched)?;
            matches.push(item);
            let candidate = search_json(
                value,
                &matches,
                &omissions,
                next_match_offset(value, offset, matches.len())?,
                next_omission_offset(value, omission_start, omissions.len(), omission_offset)?,
            );
            match candidate {
                Ok(candidate) if candidate.canonical_bytes().len() as u64 <= output_bytes => {}
                _ => {
                    matches.pop();
                    break;
                }
            }
        }
        for omission in value.omissions().get(omission_start..omission_end).unwrap_or_default() {
            omissions.push(omission_json(omission)?);
            let candidate = search_json(
                value,
                &matches,
                &omissions,
                next_match_offset(value, offset, matches.len())?,
                next_omission_offset(value, omission_start, omissions.len(), omission_offset)?,
            );
            match candidate {
                Ok(candidate) if candidate.canonical_bytes().len() as u64 <= output_bytes => {}
                _ => {
                    omissions.pop();
                    break;
                }
            }
        }
        let next_match = next_match_offset(value, offset, matches.len())?;
        let next_omission =
            next_omission_offset(value, omission_start, omissions.len(), omission_offset)?;
        if matches.is_empty()
            && omissions.is_empty()
            && (next_match == Some(offset) || next_omission == Some(omission_offset))
        {
            return Err(protocol_error());
        }
        structured = search_json(value, &matches, &omissions, next_match, next_omission)?;
        if structured.canonical_bytes().len() as u64 > output_bytes {
            return Err(protocol_error());
        }
        let more = next_match.is_some() || next_omission.is_some();
        let text = format!(
            "Found {} literal matches on this page across {} UTF-8 files ({} bytes scanned){}.",
            matches.len(),
            value.scanned_files(),
            value.scanned_bytes(),
            if more { " More matches continue" } else { "" }
        );
        finish(structured, text.clone(), text, more)
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
        ("size", u64_integer(value.size())),
    ])
}

const fn kind_name(value: &MetadataObservation) -> &'static str {
    match value.kind() {
        peritus_workspace::WorkspaceEntryKind::File => "file",
        peritus_workspace::WorkspaceEntryKind::Directory => "directory",
        peritus_workspace::WorkspaceEntryKind::Other => "other",
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

fn integer(value: i64) -> BoundedJson {
    BoundedJson::integer(value)
}

fn u64_integer(value: u64) -> Result<BoundedJson, FsToolError> {
    i64::try_from(value).map(integer).map_err(|_| protocol_error())
}

fn discover_json_with_page(
    value: &DiscoverObservation,
    entries: &[BoundedJson],
    end: u64,
    more: bool,
) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("digest", string(digest_hex(value.digest()))),
        ("entries", array(entries.to_vec())),
        ("omission_count", u64_integer(value.omission_count())),
        ("next_offset", if more { u64_integer(end) } else { Ok(BoundedJson::null()) }),
        ("observed_count", u64_integer(value.observed_count())),
        (
            "root",
            value.root().map_or_else(|| Ok(BoundedJson::null()), |path| string(path.to_string())),
        ),
        ("truncated", Ok(BoundedJson::boolean(more))),
    ])
}

fn search_json(
    value: &SearchObservation,
    matches: &[BoundedJson],
    omissions: &[BoundedJson],
    next_match: Option<u64>,
    next_omission: Option<u64>,
) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("digest", string(digest_hex(value.digest()))),
        ("match_count", u64_integer(value.match_count())),
        ("matches", array(matches.to_vec())),
        ("next_match_offset", next_match.map_or_else(|| Ok(BoundedJson::null()), u64_integer)),
        (
            "next_omission_offset",
            next_omission.map_or_else(|| Ok(BoundedJson::null()), u64_integer),
        ),
        ("omission_count", u64_integer(value.omission_count())),
        ("omissions", array(omissions.to_vec())),
        ("scanned_bytes", u64_integer(value.scanned_bytes())),
        ("scanned_files", u64_integer(value.scanned_files())),
        ("truncated", Ok(BoundedJson::boolean(next_match.is_some() || next_omission.is_some()))),
    ])
}

fn next_match_offset(
    value: &SearchObservation,
    offset: u64,
    rendered: usize,
) -> Result<Option<u64>, FsToolError> {
    let end = offset
        .checked_add(u64::try_from(rendered).map_err(|_| protocol_error())?)
        .ok_or_else(protocol_error)?;
    let page_end = value
        .page_start()
        .checked_add(u64::try_from(value.matches().len()).map_err(|_| protocol_error())?)
        .ok_or_else(protocol_error)?;
    if end < page_end { Ok(Some(end)) } else { Ok(value.next_offset()) }
}

fn next_omission_offset(
    value: &SearchObservation,
    start: usize,
    rendered: usize,
    offset: u64,
) -> Result<Option<u64>, FsToolError> {
    if start.saturating_add(rendered) < value.omissions().len() {
        Ok(Some(
            offset
                .checked_add(u64::try_from(rendered).map_err(|_| protocol_error())?)
                .ok_or_else(protocol_error)?,
        ))
    } else {
        Ok(value.next_omission_offset())
    }
}

fn omission_json(value: &ScopeOmission) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("path", string(value.path().to_string())),
        ("reason", string(omission_name(value.reason()).to_owned())),
    ])
}

const fn omission_name(value: OmissionReason) -> &'static str {
    match value {
        OmissionReason::DepthLimit => "depth_limit",
        OmissionReason::UnsafeEntry => "unsafe_entry",
        OmissionReason::FileByteLimit => "file_byte_limit",
        OmissionReason::BinaryContent => "binary_content",
    }
}

fn search_match_json(value: &crate::SearchMatch) -> Result<BoundedJson, FsToolError> {
    object(vec![
        ("column_bytes", u64_integer(value.column_bytes())),
        ("line", u64_integer(value.line())),
        ("path", string(value.path().to_string())),
        ("preview", string(value.preview().to_owned())),
    ])
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
