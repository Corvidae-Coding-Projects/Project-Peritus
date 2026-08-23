//! Canonical direct-cause links and ancestry validation.

use crate::{EvidenceError, EvidenceErrorKind, EvidenceId, EvidenceRecord, RecoveryAction};
use std::collections::BTreeMap;

/// Immutable directed link from older parent evidence to a child.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CausalLink {
    parent: EvidenceId,
    child: EvidenceId,
}

impl CausalLink {
    /// Creates a non-reflexive causal link.
    ///
    /// # Errors
    ///
    /// Rejects a self-link.
    pub fn new(parent: EvidenceId, child: EvidenceId) -> Result<Self, EvidenceError> {
        if parent == child {
            Err(invalid("evidence cannot cause itself"))
        } else {
            Ok(Self { parent, child })
        }
    }
    /// Returns the parent evidence identity.
    #[must_use]
    pub const fn parent(self) -> EvidenceId {
        self.parent
    }
    /// Returns the child evidence identity.
    #[must_use]
    pub const fn child(self) -> EvidenceId {
        self.child
    }
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "the private causality module shares this pure validator with admission"
)]
pub(crate) fn validate_parents(
    child_id: EvidenceId,
    child_position: u64,
    causes: &[EvidenceId],
    existing: &BTreeMap<EvidenceId, EvidenceRecord>,
) -> Result<Vec<CausalLink>, EvidenceError> {
    let mut links = Vec::with_capacity(causes.len());
    for cause in causes {
        let parent = existing.get(cause).ok_or_else(|| invalid("causal parent does not exist"))?;
        if !crate::verified::causal_position(parent.provenance().global_position(), child_position)
        {
            return Err(invalid("causal parent is not strictly older than child"));
        }
        links.push(CausalLink::new(*cause, child_id)?);
    }
    Ok(links)
}

fn invalid(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::InvalidCause,
        RecoveryAction::CorrectInput,
        "validate evidence ancestry",
        detail,
    )
}
