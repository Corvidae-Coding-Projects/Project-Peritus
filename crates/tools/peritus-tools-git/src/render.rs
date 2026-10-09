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
const MAX_RENDER_PARENTS: usize = 32;
const MAX_PATCH_WINDOW_BYTES: usize = 48 * 1_024;

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
                    ("path", string(entry.path().to_owned())),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("detached", Ok(BoundedJson::boolean(value.is_detached()))),
            ("digest", string(digest_hex(value.digest()))),
            ("entries", array(entries)),
            ("entry_count", Ok(integer(usize_integer(value.entries().len())))),
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

    /// Renders structured changed paths plus an exact bounded patch window.
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
            ("entry_count", Ok(integer(usize_integer(value.entries().len())))),
            ("patch_base64", string(STANDARD.encode(patch))),
            ("patch_bytes", Ok(integer(usize_integer(value.patch().len())))),
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

    /// Renders bounded commit and parent identities.
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
                    ("timestamp_seconds", Ok(integer(u64_integer(commit.timestamp_seconds())))),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        let structured = object(vec![
            ("commit_count", Ok(integer(usize_integer(value.commits().len())))),
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
