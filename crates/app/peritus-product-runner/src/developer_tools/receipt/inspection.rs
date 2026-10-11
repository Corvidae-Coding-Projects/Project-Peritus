//! Read-only uncertain-effect projection and explicit human review.

use sha2::{Digest as _, Sha256};

use super::{
    EffectReceiptLedger, ReceiptRecord, ReceiptState, command_effect, hex, receipt_revision,
    storage,
};

/// Read-only identity of a command effect whose outcome cannot safely be replayed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UncertainEffect {
    identity: String,
    tool: String,
    state: UncertainEffectState,
    requirements_revision: Option<u64>,
    owner_inactive: bool,
}

/// Durable receipt state for a command without a terminal result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UncertainEffectState {
    /// The command was admitted and may still be running under the live run owner.
    Started,
    /// Recovery proved only that the command may have taken effect, so replay is forbidden.
    Ambiguous,
    /// The unknown outcome was reviewed and now freezes replacement effects for its requirements.
    Reviewed,
}

impl UncertainEffect {
    /// Stable receipt identity of the original provider operation.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Developer command tool that may already have taken effect.
    #[must_use]
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// Current durable receipt state.
    #[must_use]
    pub const fn state(&self) -> UncertainEffectState {
        self.state
    }

    /// Requirements revision that owns this effect when encoded by the production runner.
    #[must_use]
    pub const fn requirements_revision(&self) -> Option<u64> {
        self.requirements_revision
    }

    /// Returns whether exact native-owner reconciliation durably proved the process inactive.
    #[must_use]
    pub const fn owner_inactive(&self) -> bool {
        self.owner_inactive
    }
}

/// Inspects durable receipt owners without advancing, replaying, or repairing any effect.
///
/// # Errors
/// Rejects an unreadable, oversized, malformed, or conflicting receipt history.
pub fn uncertain_effects(
    path: &std::path::Path,
) -> Result<Vec<UncertainEffect>, crate::ProductRunnerError> {
    Ok(inspect_records(path)?
        .into_iter()
        .filter(|record| {
            matches!(
                record.state,
                ReceiptState::Started | ReceiptState::Ambiguous | ReceiptState::Reviewed
            ) && command_effect(&record.tool)
        })
        .map(|record| UncertainEffect {
            identity: uncertain_identity(&record),
            tool: record.tool,
            state: match record.state {
                ReceiptState::Started => UncertainEffectState::Started,
                ReceiptState::Ambiguous => UncertainEffectState::Ambiguous,
                ReceiptState::Reviewed => UncertainEffectState::Reviewed,
                _ => unreachable!("filtered uncertain receipt state"),
            },
            requirements_revision: receipt_revision(&record.scope),
            owner_inactive: record.owner_inactive,
        })
        .collect())
}

/// Records explicit human review of one unprovable command outcome without replaying it.
///
/// The retained result is an error observation: it never claims that the command succeeded or
/// failed, and an exact retry replays that observation instead of invoking the command again.
/// The caller must first prove that the run owner is no longer actively executing this receipt.
///
/// # Errors
/// Rejects an unreadable or invalid ledger, an identity that is no longer the current uncertain
/// receipt, or a receipt that does not describe a command effect.
pub fn acknowledge_uncertain_effect(
    path: &std::path::Path,
    identity: &str,
) -> Result<(), crate::ProductRunnerError> {
    let records = inspect_records(path)?;
    let Some(record) = records.into_iter().find(|record| {
        command_effect(&record.tool)
            && matches!(record.state, ReceiptState::Started | ReceiptState::Ambiguous)
            && record.owner_inactive
            && uncertain_identity(record) == identity
    }) else {
        return Err(inspect_error("the command receipt is no longer awaiting explicit review"));
    };
    let reviewed = ReceiptRecord {
        state: ReceiptState::Reviewed,
        output: Some(serde_json::json!({
            "error": "Outcome manually reviewed by the user. No success or failure was inferred, and the command was not repeated.",
            "reviewed": true,
            "outcome_unknown": true,
        })),
        is_error: Some(true),
        ..record
    };
    EffectReceiptLedger::new(path.to_path_buf(), reviewed.scope.clone())
        .append(&reviewed)
        .map_err(|error| inspect_error(error.to_string()))
}

fn inspect_records(
    path: &std::path::Path,
) -> Result<Vec<ReceiptRecord>, crate::ProductRunnerError> {
    // A trailing partial frame was never durable. The mutating ledger loader truncates it before
    // the next append; read-only projection ignores it without changing the file.
    storage::inspect(path).map_err(|error| inspect_error(error.to_string()))
}

fn uncertain_identity(record: &ReceiptRecord) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-command-receipt-v1");
    for bytes in [
        record.scope.as_bytes(),
        record.call_id.as_bytes(),
        record.tool.as_bytes(),
        record.request_sha256.as_bytes(),
    ] {
        hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.update(record.ordinal.to_le_bytes());
    format!("command/{}", hex(hasher.finalize().into()))
}

fn inspect_error(detail: impl Into<String>) -> crate::ProductRunnerError {
    crate::ProductRunnerError::new(
        crate::ProductRunnerErrorKind::InvalidPrecondition,
        "inspect developer effect receipts",
        detail.into(),
    )
}
