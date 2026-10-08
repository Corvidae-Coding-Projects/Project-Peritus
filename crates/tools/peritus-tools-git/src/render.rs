//! Bounded structured, model, and human Git renderings.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use peritus_git::{
    DiffChange, GitDiffObservation, GitHistoryObservation, StatusKind, StatusObservation,
};
use peritus_tool_protocol::{BoundedJson, BoundedText, JsonLimits, Truncation};

use crate::{
    DiffObservationPage, GitToolError, GitToolErrorKind, GitToolOperation,
    HistoryObservationPage, RecoveryClass, RetainedSnapshotObservation, SnapshotObservation,
    StatusObservationPage,
};

const MAX_RENDER_ITEMS: usize = 500;
const MAX_RENDER_PARENTS: usize = 32;
const MAX_PATCH_WINDOW_BYTES: usize = 48 * 1_024;

/// Independently bounded structured, model, and human Git rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedOutput {
    structured: BoundedJson,
    model: BoundedText,
    human: BoundedText,
    output_truncation: Truncation,
}

impl RenderedOutput {
    /// Returns the exact canonical JSON byte length charged to the selected output envelope.
    #[must_use]
    pub fn encoded_bytes(&self) -> usize {
        self.structured.canonical_bytes().len()
    }

    /// Renders a bounded continuation when one indivisible record needs a larger envelope.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn deferred(
        operation: &'static str,
        cursor: &str,
        minimum_output_bytes: usize,
    ) -> Result<Self, GitToolError> {
        let structured = object(vec![
            ("coverage_complete", Ok(BoundedJson::boolean(false))),
            ("minimum_output_bytes", unsigned_usize(minimum_output_bytes)),
            ("next_cursor", string(cursor.to_owned())),
            ("operation", string(operation.to_owned())),
            ("retry_same_page", Ok(BoundedJson::boolean(true))),
            ("truncated", Ok(BoundedJson::boolean(true))),
        ])?;
        finish_with_truncation(
            structured,
            format!(
                "{operation} requires at least {minimum_output_bytes} output bytes for its next physical record."
            ),
            Truncation::Indeterminate,
        )
    }

    /// Renders the compact exact key for a target-owned durable repository result.
    ///
    /// The fixed-width key is intentionally renderable before the Git effect; a replacement
    /// process uses it to adopt the completed result or enter reconciliation without replay.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn mutation_receipt(
        operation_digest: peritus_types::Sha256Digest,
    ) -> Result<Self, GitToolError> {
        let receipt = digest_hex(operation_digest);
        let structured = object(vec![("mutation_receipt", string(receipt.clone()))])?;
        finish(structured, format!("Git mutation receipt {receipt}."), false)
    }

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

    /// Renders exact status identities and a bounded legacy entry window.
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
                    ("path", string(entry.path().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("detached", Ok(BoundedJson::boolean(value.is_detached()))),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("entry_count", unsigned_usize(value.entries().len())),
            ("head", string(value.head().to_string())),
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

    /// Renders one exact bounded page of immutable status.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn status_page(value: &StatusObservationPage) -> Result<Self, GitToolError> {
        let observation = value.observation();
        let entries = value
            .entries()
            .iter()
            .map(|entry| {
                object(vec![
                    ("kind", string(status_kind(entry.kind()).to_owned())),
                    ("path", string(entry.path().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let has_prior = value.entry_start() > 0;
        let has_next = value.next_cursor().is_some();
        let structured = object(vec![
            ("coverage_complete", Ok(BoundedJson::boolean(!has_next))),
            ("cursor", string(value.cursor().to_owned())),
            ("detached", Ok(BoundedJson::boolean(observation.is_detached()))),
            ("digest", string(digest_hex(observation.digest()))),
            ("entries", array(entries)),
            ("entry_count", unsigned_usize(observation.entries().len())),
            ("entry_range_end", Ok(BoundedJson::unsigned(value.entry_end()))),
            ("entry_range_start", Ok(BoundedJson::unsigned(value.entry_start()))),
            ("head", string(observation.head().to_string())),
            (
                "index_tree",
                observation
                    .index_tree()
                    .map_or_else(|| Ok(BoundedJson::null()), |tree| string(tree.to_string())),
            ),
            ("next_cursor", optional_string(value.next_cursor())),
            (
                "repository_digest",
                string(digest_hex(observation.repository_digest())),
            ),
            ("returned_entry_count", unsigned_usize(value.entries().len())),
            ("truncated", Ok(BoundedJson::boolean(has_prior || has_next))),
        ])?;
        finish_page(
            structured,
            format!(
                "Git status entries {}..{} of {}; detached={}.",
                value.entry_start(),
                value.entry_end(),
                observation.entries().len(),
                observation.is_detached()
            ),
            has_prior,
            has_next,
        )
    }

    /// Renders structured changed paths plus a bounded legacy patch prefix.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn diff(value: &GitDiffObservation) -> Result<Self, GitToolError> {
        let retained = value.entries().len().min(MAX_RENDER_ITEMS);
        let patch_retained = value.patch().len().min(MAX_PATCH_WINDOW_BYTES);
        let truncated = retained < value.entries().len() || patch_retained < value.patch().len();
        let entries = value.entries()[..retained]
            .iter()
            .map(|entry| {
                object(vec![
                    ("change", string(change_name(entry.change()).to_owned())),
                    ("path", string(entry.path().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let patch = &value.patch()[..patch_retained];
        let structured = object(vec![
            ("base", string(value.base().to_string())),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("entry_count", unsigned_usize(value.entries().len())),
            ("patch_base64", string(STANDARD.encode(patch))),
            ("patch_bytes", unsigned_usize(value.patch().len())),
            ("target", string(value.target().to_string())),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ])?;
        finish(
            structured,
            format!(
                "Git diff contains {} changed paths and {} patch bytes.",
                value.entries().len(),
                value.patch().len()
            ),
            truncated,
        )
    }

    /// Renders one exact changed-path or patch-byte page of an immutable diff.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn diff_page(value: &DiffObservationPage) -> Result<Self, GitToolError> {
        let observation = value.observation();
        let entries = value
            .entries()
            .iter()
            .map(|entry| {
                object(vec![
                    ("change", string(change_name(entry.change()).to_owned())),
                    ("path", string(entry.path().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let has_prior = value.entry_start() > 0 || value.patch_start() > 0;
        let has_next = value.next_cursor().is_some();
        let structured = object(vec![
            ("base", string(observation.base().to_string())),
            ("coverage_complete", Ok(BoundedJson::boolean(!has_next))),
            ("cursor", string(value.cursor().to_owned())),
            ("digest", string(digest_hex(observation.digest()))),
            ("entries", array(entries)),
            ("entry_count", unsigned_usize(observation.entries().len())),
            ("entry_range_end", Ok(BoundedJson::unsigned(value.entry_end()))),
            ("entry_range_start", Ok(BoundedJson::unsigned(value.entry_start()))),
            ("next_cursor", optional_string(value.next_cursor())),
            ("patch_base64", string(STANDARD.encode(value.patch()))),
            ("patch_bytes", unsigned_usize(observation.patch().len())),
            ("patch_range_end", Ok(BoundedJson::unsigned(value.patch_end()))),
            ("patch_range_start", Ok(BoundedJson::unsigned(value.patch_start()))),
            (
                "repository_digest",
                string(digest_hex(observation.repository_digest())),
            ),
            ("returned_entry_count", unsigned_usize(value.entries().len())),
            ("returned_patch_bytes", unsigned_usize(value.patch().len())),
            ("target", string(observation.target().to_string())),
            ("truncated", Ok(BoundedJson::boolean(has_prior || has_next))),
        ])?;
        finish_page(
            structured,
            format!(
                "Git diff paths {}..{} of {}; patch bytes {}..{} of {}.",
                value.entry_start(),
                value.entry_end(),
                observation.entries().len(),
                value.patch_start(),
                value.patch_end(),
                observation.patch().len()
            ),
            has_prior,
            has_next,
        )
    }

    /// Renders a bounded legacy commit and parent prefix.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn history(value: &GitHistoryObservation) -> Result<Self, GitToolError> {
        let retained = value.commits().len().min(MAX_RENDER_ITEMS);
        let mut truncated = retained < value.commits().len();
        let commits = value.commits()[..retained]
            .iter()
            .map(|commit| {
                let retained_parents = commit.parents().len().min(MAX_RENDER_PARENTS);
                truncated |= retained_parents < commit.parents().len();
                let parents = commit.parents()[..retained_parents]
                    .iter()
                    .map(|parent| string(parent.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                object(vec![
                    ("commit", string(commit.commit().to_string())),
                    ("parents", array(parents)),
                    ("subject", string(commit.subject().to_owned())),
                    ("timestamp_seconds", Ok(BoundedJson::unsigned(commit.timestamp_seconds()))),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("commit_count", unsigned_usize(value.commits().len())),
            ("commits", array(commits)),
            ("digest", string(digest_hex(value.digest()))),
            ("start", string(value.start().to_string())),
            ("truncated", Ok(BoundedJson::boolean(truncated))),
        ])?;
        finish(
            structured,
            format!("Git history contains {} observed commits.", value.commits().len()),
            truncated,
        )
    }

    /// Renders one exact commit/parent page of immutable history.
    ///
    /// # Errors
    /// Returns a typed protocol-bound failure.
    pub fn history_page(value: &HistoryObservationPage) -> Result<Self, GitToolError> {
        let observation = value.observation();
        let commits = value
            .commit()
            .map(|commit| {
                let parents = value
                    .parents()
                    .iter()
                    .map(|parent| string(parent.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                object(vec![
                    ("commit", string(commit.commit().to_string())),
                    ("commit_index", Ok(BoundedJson::unsigned(value.commit_index()))),
                    ("parent_count", unsigned_usize(commit.parents().len())),
                    ("parent_range_end", Ok(BoundedJson::unsigned(value.parent_end()))),
                    ("parent_range_start", Ok(BoundedJson::unsigned(value.parent_start()))),
                    ("parents", array(parents)),
                    ("subject", string(commit.subject().to_owned())),
                    (
                        "timestamp_seconds",
                        Ok(BoundedJson::unsigned(commit.timestamp_seconds())),
                    ),
                ])
            })
            .transpose()?
            .into_iter()
            .collect::<Vec<_>>();
        let has_prior = value.commit_index() > 0 || value.parent_start() > 0;
        let has_next = value.next_cursor().is_some();
        let returned = if value.commit().is_some() { 1_u64 } else { 0 };
        let new_commits = if value.parent_start() == 0 { returned } else { 0 };
        let structured = object(vec![
            ("commit_count", unsigned_usize(observation.commits().len())),
            ("commit_range_end", Ok(BoundedJson::unsigned(value.commit_index() + returned))),
            ("commit_range_start", Ok(BoundedJson::unsigned(value.commit_index()))),
            ("commits", array(commits)),
            ("coverage_complete", Ok(BoundedJson::boolean(!has_next))),
            ("cursor", string(value.cursor().to_owned())),
            ("digest", string(digest_hex(observation.digest()))),
            ("next_cursor", optional_string(value.next_cursor())),
            (
                "repository_digest",
                string(digest_hex(observation.repository_digest())),
            ),
            ("returned_commit_count", Ok(BoundedJson::unsigned(new_commits))),
            ("returned_parent_count", unsigned_usize(value.parents().len())),
            ("start", string(observation.start().to_string())),
            ("truncated", Ok(BoundedJson::boolean(has_prior || has_next))),
        ])?;
        finish_page(
            structured,
            format!(
                "Git history commit {} of {}; parent identities {}..{}.",
                value.commit_index(),
                observation.commits().len(),
                value.parent_start(),
                value.parent_end()
            ),
            has_prior,
            has_next,
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
            ("generation", Ok(BoundedJson::unsigned(value.generation().get()))),
            ("revision", Ok(BoundedJson::unsigned(value.revision().get()))),
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
    /// Returns whether the structured output omits any part of the complete observation.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        !matches!(self.output_truncation, Truncation::Complete)
    }
    /// Returns truthful placement of this output in the complete observation.
    #[must_use]
    pub const fn output_truncation(&self) -> Truncation {
        self.output_truncation
    }
}

fn finish(
    structured: BoundedJson,
    text: String,
    truncated: bool,
) -> Result<RenderedOutput, GitToolError> {
    finish_with_truncation(
        structured,
        text,
        if truncated { Truncation::TailDropped } else { Truncation::Complete },
    )
}

fn finish_page(
    structured: BoundedJson,
    text: String,
    has_prior: bool,
    has_next: bool,
) -> Result<RenderedOutput, GitToolError> {
    let truncation = match (has_prior, has_next) {
        (false, false) => Truncation::Complete,
        (false, true) => Truncation::TailDropped,
        (true, false) => Truncation::HeadDropped,
        (true, true) => Truncation::Windowed,
    };
    finish_with_truncation(structured, text, truncation)
}

fn finish_with_truncation(
    structured: BoundedJson,
    text: String,
    output_truncation: Truncation,
) -> Result<RenderedOutput, GitToolError> {
    let model = BoundedText::new(text.clone()).map_err(|_| protocol_error())?;
    let human = BoundedText::new(text).map_err(|_| protocol_error())?;
    Ok(RenderedOutput { structured, model, human, output_truncation })
}

fn object(
    members: Vec<(&str, Result<BoundedJson, GitToolError>)>,
) -> Result<BoundedJson, GitToolError> {
    let members = members
        .into_iter()
        .map(|(name, value)| value.map(|value| (name.to_owned(), value)))
        .collect::<Result<Vec<_>, _>>()?;
    BoundedJson::object(members, JsonLimits::MAXIMUM).map_err(|_| protocol_error())
}

fn array(values: Vec<BoundedJson>) -> Result<BoundedJson, GitToolError> {
    BoundedJson::array(values, JsonLimits::MAXIMUM).map_err(|_| protocol_error())
}

fn string(value: String) -> Result<BoundedJson, GitToolError> {
    BoundedJson::string(value, JsonLimits::MAXIMUM).map_err(|_| protocol_error())
}

fn optional_string(value: Option<&str>) -> Result<BoundedJson, GitToolError> {
    value.map_or_else(|| Ok(BoundedJson::null()), |value| string(value.to_owned()))
}

fn unsigned_usize(value: usize) -> Result<BoundedJson, GitToolError> {
    u64::try_from(value)
        .map(BoundedJson::unsigned)
        .map_err(|_| protocol_error())
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

const fn protocol_error() -> GitToolError {
    GitToolError::new(
        GitToolErrorKind::Protocol,
        GitToolOperation::Catalog,
        RecoveryClass::CorrectInput,
        "Git tool output exceeded the bounded protocol",
    )
}
