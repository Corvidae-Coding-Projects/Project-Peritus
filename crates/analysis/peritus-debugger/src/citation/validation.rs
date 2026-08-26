//! Canonical citation-set validation.

use crate::{DebuggerError, EvidenceCitation, TraceSelectionManifest};

/// Validates a nonempty, strictly ordered citation set against one manifest.
///
/// # Errors
///
/// Rejects empty, duplicate, descending, or non-contained citations.
pub fn validate_citations(
    citations: &[EvidenceCitation],
    manifest: &TraceSelectionManifest,
) -> Result<(), DebuggerError> {
    if citations.is_empty() || citations.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(DebuggerError::new(
            crate::DebuggerErrorKind::Citation,
            crate::DebuggerOperation::ValidateCitation,
            crate::DebuggerRecovery::CorrectInput,
            "citation set must be nonempty, strictly ordered, and unique",
        ));
    }
    citations.iter().try_for_each(|citation| citation.validate_against(manifest))
}
