//! Bounded structured, model, and human Git renderings.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_git::{
    DiffChange, GitDiffObservation, GitHistoryObservation, StatusKind, StatusObservation,
};
use peritus_tool_protocol::{BoundedJson, BoundedText, JsonLimits};

use crate::{
    GitToolError, GitToolErrorKind, GitToolOperation, RecoveryClass, RetainedSnapshotObservation,
    SnapshotObservation,
};

const MAX_RENDER_ITEMS: usize = 500;
mod diff;
const MAX_HISTORY_PAGE_BYTES: usize = 64 * 1_024;
const MAX_HISTORY_SUBJECT_PAGE_BYTES: usize = 64;

/// Independently bounded structured, model, and human Git rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedOutput {
    structured: BoundedJson,
    model: BoundedText,
    human: BoundedText,
    truncated: bool,
}

impl RenderedOutput {
    /// Renders an authorized candidate-plus-snapshot outcome.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn candidate(value: &peritus_workspace::CandidateOutcome) -> Result<Self, GitToolError> {
        let structured = object(vec![
            ("artifact_digest", string(digest_hex(value.artifact_digest().sha256()))),
            ("commit", string(value.snapshot().commit().to_string())),
            ("manifest_digest", string(digest_hex(value.snapshot().manifest_digest()))),
            ("patch_identity", string(value.patch_id().to_string())),
            ("snapshot_id", string(identifier_hex(value.snapshot().snapshot_id().as_bytes()))),
            ("tree", string(value.snapshot().tree().to_string())),
        ])?;
        finish(
            structured,
            format!(
                "Created retained candidate snapshot {}.",
                identifier_hex(value.snapshot().snapshot_id().as_bytes())
            ),
            false,
        )
    }

    /// Renders an authorized history-preserving rollback outcome.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn rollback(value: &peritus_workspace::RollbackOutcome) -> Result<Self, GitToolError> {
        let structured = object(vec![
            ("artifact_digest", string(digest_hex(value.artifact_digest().sha256()))),
            ("commit", string(value.snapshot().commit().to_string())),
            ("restored_from", string(value.restored_from().to_string())),
            ("snapshot_id", string(identifier_hex(value.snapshot().snapshot_id().as_bytes()))),
            ("tree", string(value.snapshot().tree().to_string())),
        ])?;
        finish(
            structured,
            format!(
                "Restored {} as successor snapshot {}.",
                value.restored_from(),
                identifier_hex(value.snapshot().snapshot_id().as_bytes())
            ),
            false,
        )
    }

    /// Renders exact status identities and a bounded entry window.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn status(value: &StatusObservation) -> Result<Self, GitToolError> {
        let retained = value.entries().len().min(MAX_RENDER_ITEMS);
        let truncated = retained < value.entries().len();
        let entries = value.entries()[..retained]
            .iter()
            .map(|entry| {
                object(vec![
                    ("kind", string(status_kind(entry.kind()).to_owned())),
                    ("path", string(String::from_utf8_lossy(entry.path()).into_owned())),
                    ("path_bytes_base64", string(STANDARD.encode(entry.path()))),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("detached", Ok(BoundedJson::boolean(value.is_detached()))),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("entry_count", Ok(integer(usize_integer(value.entries().len())))),
            (
                "head",
                value
                    .head()
                    .map_or_else(|| Ok(BoundedJson::null()), |head| string(head.to_string())),
            ),
            (
                "index_tree",
                value
                    .index_tree()
                    .map_or_else(|| Ok(BoundedJson::null()), |tree| string(tree.to_string())),
            ),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ])?;
        finish(
            structured,
            format!(
                "Git status has {} entries; detached={}.",
                value.entries().len(),
                value.is_detached()
            ),
            truncated,
        )
    }

    /// Renders structured changed paths plus an exact bounded patch window.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn diff(value: &GitDiffObservation) -> Result<Self, GitToolError> {
        Self::diff_with_budget(value, 64 * 1024)
    }

    /// Renders a continuable diff page within the selected encoded output budget.
    ///
    /// # Errors
    /// Rejects a budget too small for the source binding and one unit of progress.
    pub fn diff_with_budget(value: &GitDiffObservation, budget: u64) -> Result<Self, GitToolError> {
        diff::render(value, budget)
    }

    /// Renders bounded commit and parent identities.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn history(value: &GitHistoryObservation) -> Result<Self, GitToolError> {
        Self::history_with_budget(value, MAX_HISTORY_PAGE_BYTES as u64)
    }

    /// Renders a history page within the caller's exact structured-output budget.
    ///
    /// # Errors
    /// Rejects a budget too small for the history cursor envelope and one commit row.
    #[allow(
        clippy::too_many_lines,
        reason = "history rows and byte cursors are budgeted against the complete canonical envelope"
    )]
    pub fn history_with_budget(
        value: &GitHistoryObservation,
        budget: u64,
    ) -> Result<Self, GitToolError> {
        let budget = usize::try_from(budget).unwrap_or(usize::MAX).min(MAX_HISTORY_PAGE_BYTES);
        let mut commits = Vec::new();
        let mut encoded_bytes = 0_usize;
        let mut next_subject_offset = None;
        for (index, commit) in value.commits().iter().take(MAX_RENDER_ITEMS).enumerate() {
            let subject_offset = if index == 0 { value.subject_offset() } else { 0 };
            let subject_start = usize::try_from(subject_offset).map_err(|_| protocol_error())?;
            let subject = commit.subject().get(subject_start..).ok_or_else(protocol_error)?;
            let mut subject_end = subject.len().min(MAX_HISTORY_SUBJECT_PAGE_BYTES);
            while !subject.is_char_boundary(subject_end) {
                subject_end -= 1;
            }
            let subject_chunk = &subject[..subject_end];
            next_subject_offset = (subject_end < subject.len())
                .then(|| subject_offset.checked_add(u32::try_from(subject_end).unwrap_or(u32::MAX)))
                .flatten();
            let parents = commit
                .parents()
                .iter()
                .map(|parent| string(parent.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let row = object(vec![
                ("commit", string(commit.commit().to_string())),
                (
                    "next_parent_offset",
                    commit.next_parent_offset().map_or_else(
                        || Ok(BoundedJson::null()),
                        |offset| Ok(integer(i64::from(offset))),
                    ),
                ),
                ("parent_count", Ok(integer(i64::from(commit.parent_count())))),
                ("parent_offset", Ok(integer(i64::from(commit.parent_offset())))),
                ("parents", array(parents)),
                ("subject", string(subject_chunk.to_owned())),
                ("subject_offset", Ok(integer(i64::from(subject_offset)))),
                (
                    "next_subject_offset",
                    next_subject_offset.map_or_else(
                        || Ok(BoundedJson::null()),
                        |offset| Ok(integer(i64::from(offset))),
                    ),
                ),
                ("timestamp_seconds", Ok(integer(u64_integer(commit.timestamp_seconds())))),
            ])?;
            let row_bytes = row.canonical_bytes().len();
            if encoded_bytes.saturating_add(row_bytes) > MAX_HISTORY_PAGE_BYTES {
                if commits.is_empty() {
                    return Err(protocol_error());
                }
                break;
            }
            encoded_bytes = encoded_bytes.saturating_add(row_bytes);
            commits.push(row);
            if next_subject_offset.is_some() {
                break;
            }
        }
        let encode_page = |rows: &[BoundedJson],
                           parent_truncated: bool,
                           next_subject_offset: Option<u32>| {
            let retained = rows.len();
            let truncated = retained < value.commits().len()
                || value.next_offset().is_some()
                || parent_truncated
                || next_subject_offset.is_some();
            let next_offset = if next_subject_offset.is_some() {
                value.offset().checked_add(retained.saturating_sub(1) as u64)
            } else if retained < value.commits().len() {
                value.offset().checked_add(retained as u64)
            } else {
                value.next_offset()
            };
            object(vec![
                ("commit_count", Ok(integer(usize_integer(value.commits().len())))),
                ("commits", array(rows.to_vec())),
                ("digest", string(digest_hex(value.digest()))),
                (
                    "next_offset",
                    next_offset.map_or_else(
                        || Ok(BoundedJson::null()),
                        |offset| i64::try_from(offset).map(integer).map_err(|_| protocol_error()),
                    ),
                ),
                (
                    "offset",
                    Ok(integer(i64::try_from(value.offset()).map_err(|_| protocol_error())?)),
                ),
                ("parent_offset", Ok(integer(i64::from(value.parent_offset())))),
                ("subject_offset", Ok(integer(i64::from(value.subject_offset())))),
                (
                    "next_subject_offset",
                    next_subject_offset.map_or_else(
                        || Ok(BoundedJson::null()),
                        |offset| Ok(integer(i64::from(offset))),
                    ),
                ),
                ("start", string(value.start().to_string())),
                ("truncated", Ok(BoundedJson::boolean(truncated))),
            ])
        };
        let retained = loop {
            let page_parent_truncated = value
                .commits()
                .iter()
                .take(commits.len())
                .any(|commit| commit.next_parent_offset().is_some());
            let structured = encode_page(&commits, page_parent_truncated, next_subject_offset)?;
            if structured.canonical_bytes().len() <= budget {
                break commits.len();
            }
            if commits.pop().is_none() {
                return Err(protocol_error());
            }
            next_subject_offset = None;
            if commits.is_empty() && !value.commits().is_empty() {
                return Err(protocol_error());
            }
        };
        let truncated = retained < value.commits().len()
            || value.next_offset().is_some()
            || value
                .commits()
                .iter()
                .take(retained)
                .any(|commit| commit.next_parent_offset().is_some());
        let truncated = truncated || next_subject_offset.is_some();
        let structured = encode_page(
            &commits,
            value
                .commits()
                .iter()
                .take(retained)
                .any(|commit| commit.next_parent_offset().is_some()),
            next_subject_offset,
        )?;
        finish(
            structured,
            format!("Git history contains {} observed commits.", value.commits().len()),
            truncated,
        )
    }

    /// Renders current C1 snapshot identity.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn snapshot(value: &SnapshotObservation) -> Result<Self, GitToolError> {
        let structured = object(vec![
            ("commit", string(value.commit().to_string())),
            ("digest", string(digest_hex(value.digest()))),
            ("generation", Ok(integer(u64_integer(value.generation().get())))),
            ("revision", Ok(integer(u64_integer(value.revision().get())))),
            ("tree", string(value.tree().to_string())),
            ("workspace_id", string(identifier_hex(value.workspace_id().as_bytes()))),
        ])?;
        finish(
            structured,
            format!("Current workspace snapshot is commit {}.", value.commit()),
            false,
        )
    }

    /// Renders retained candidate-snapshot metadata.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn retained_snapshot(value: &RetainedSnapshotObservation) -> Result<Self, GitToolError> {
        let structured = object(vec![
            ("commit", string(value.commit().to_string())),
            ("manifest_digest", string(digest_hex(value.manifest_digest()))),
            ("reference", string(value.reference().to_owned())),
            ("snapshot_id", string(identifier_hex(value.snapshot_id().as_bytes()))),
            ("tree", string(value.tree().to_string())),
            ("workspace_id", string(identifier_hex(value.workspace_id().as_bytes()))),
        ])?;
        finish(
            structured,
            format!(
                "Retained snapshot {} is commit {}.",
                identifier_hex(value.snapshot_id().as_bytes()),
                value.commit()
            ),
            false,
        )
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
    /// Returns whether a structured output window was truncated.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

fn finish(
    structured: BoundedJson,
    text: String,
    truncated: bool,
) -> Result<RenderedOutput, GitToolError> {
    let model = BoundedText::new(text.clone()).map_err(|_| protocol_error())?;
    let human = BoundedText::new(text).map_err(|_| protocol_error())?;
    Ok(RenderedOutput { structured, model, human, truncated })
}

fn object(
    members: Vec<(&str, Result<BoundedJson, GitToolError>)>,
) -> Result<BoundedJson, GitToolError> {
    let members = members
        .into_iter()
        .map(|(name, value)| value.map(|value| (name.to_owned(), value)))
        .collect::<Result<Vec<_>, _>>()?;
    BoundedJson::object(members, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn array(values: Vec<BoundedJson>) -> Result<BoundedJson, GitToolError> {
    BoundedJson::array(values, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn string(value: String) -> Result<BoundedJson, GitToolError> {
    BoundedJson::string(value, JsonLimits::PRODUCTION).map_err(|_| protocol_error())
}

fn integer(value: i64) -> BoundedJson {
    BoundedJson::integer(value)
}

const fn status_kind(value: &StatusKind) -> &'static str {
    match value {
        StatusKind::Ordinary { .. } => "ordinary",
        StatusKind::Renamed { .. } => "renamed",
        StatusKind::Unmerged { .. } => "unmerged",
        StatusKind::Untracked => "untracked",
        StatusKind::Ignored => "ignored",
    }
}

const fn change_name(value: DiffChange) -> &'static str {
    match value {
        DiffChange::Added => "added",
        DiffChange::Modified => "modified",
        DiffChange::Deleted => "deleted",
        DiffChange::TypeChanged => "type-changed",
        DiffChange::Unmerged => "unmerged",
    }
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

fn u64_integer(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn usize_integer(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

const fn protocol_error() -> GitToolError {
    GitToolError::new(
        GitToolErrorKind::Protocol,
        GitToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "Git tool output exceeded the bounded protocol",
    )
}
