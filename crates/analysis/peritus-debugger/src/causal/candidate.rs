//! Checked root-cause, ambiguity, alternative, and unsupported values.

use crate::{
    CauseId, DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery,
    EvidenceCitation, FailureCategory, ModelAnalysisId, TraceSelectionManifest, validate_citations,
};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use super::ConfidenceMillionths;

const CAUSE_ID_DOMAIN: &[u8] = b"peritus-e2-root-cause-id-v1\0";

/// Validated bounded diagnostic prose.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticText(String);

impl DiagnosticText {
    /// Maximum UTF-8 bytes retained by one statement.
    pub const MAX_BYTES: usize = 4_096;

    /// Validates nonempty, bounded text without NUL or non-whitespace controls.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, or control-bearing text.
    pub fn new(value: impl Into<String>) -> Result<Self, DebuggerError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > Self::MAX_BYTES
            || value.chars().any(|character| {
                character == '\0' || (character.is_control() && !character.is_whitespace())
            })
        {
            Err(report_error("diagnostic text is empty, excessive, or contains controls"))
        } else {
            Ok(Self(value))
        }
    }
    /// Borrows validated text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Explicit ambiguity retained with a cause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AmbiguityFlag {
    /// A causal predecessor was outside a selected-only manifest.
    MissingCausalPredecessor,
    /// Source wall clocks disagree with deterministic order.
    ClockDisagreement,
    /// Multiple categories plausibly explain the evidence.
    MultiplePlausibleCauses,
    /// A span never reached a terminal observation.
    IncompleteSpan,
    /// Attribution crosses revisions or cannot isolate one component.
    CrossRevisionAttribution,
}

/// Explicit alternative-cause inventory.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AlternativeCauses {
    /// Analysis found no distinct alternative under the frozen analyzer rules.
    NoneKnown,
    /// Distinct categories in canonical tag order.
    Categories(Vec<FailureCategory>),
}

impl AlternativeCauses {
    pub(crate) fn validate(&self, primary: FailureCategory) -> Result<(), DebuggerError> {
        if let Self::Categories(categories) = self
            && (categories.is_empty()
                || categories.contains(&primary)
                || categories.windows(2).any(|pair| pair[0] >= pair[1]))
        {
            return Err(report_error(
                "alternative causes must be nonempty, distinct, canonical, and non-primary",
            ));
        }
        Ok(())
    }
}

/// Derivation boundary for a checked cause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CauseDerivation {
    /// Derived by the frozen deterministic analyzer registry.
    Deterministic,
    /// Added by one fully validated inert model proposal.
    ValidatedModel(ModelAnalysisId),
}

/// One checked, evidence-linked candidate cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootCauseCandidate {
    id: CauseId,
    category: FailureCategory,
    statement: DiagnosticText,
    support: Arc<[EvidenceCitation]>,
    contrary: Arc<[EvidenceCitation]>,
    alternatives: AlternativeCauses,
    confidence: ConfidenceMillionths,
    ambiguities: Vec<AmbiguityFlag>,
    derivation: CauseDerivation,
}

impl RootCauseCandidate {
    /// Validates complete cause evidence and derives a stable identity.
    ///
    /// # Errors
    ///
    /// Rejects empty/noncanonical support, duplicate contrary evidence, invalid alternatives,
    /// confidence/ambiguity disagreement, or citations outside the manifest.
    #[allow(clippy::too_many_arguments, reason = "complete causal evidence remains explicit")]
    pub fn new(
        manifest: &TraceSelectionManifest,
        category: FailureCategory,
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
        contrary: Vec<EvidenceCitation>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        ambiguities: Vec<AmbiguityFlag>,
        derivation: CauseDerivation,
    ) -> Result<Self, DebuggerError> {
        Self::new_shared(
            manifest,
            category,
            statement,
            Arc::from(support),
            Arc::from(contrary),
            alternatives,
            confidence,
            ambiguities,
            derivation,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "complete causal evidence remains explicit")]
    pub(crate) fn new_shared(
        manifest: &TraceSelectionManifest,
        category: FailureCategory,
        statement: DiagnosticText,
        support: Arc<[EvidenceCitation]>,
        contrary: Arc<[EvidenceCitation]>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        ambiguities: Vec<AmbiguityFlag>,
        derivation: CauseDerivation,
    ) -> Result<Self, DebuggerError> {
        Self::new_indexed(
            Some(manifest),
            category,
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            ambiguities,
            derivation,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "complete causal evidence remains explicit")]
    pub(crate) fn new_prevalidated(
        category: FailureCategory,
        statement: DiagnosticText,
        support: Arc<[EvidenceCitation]>,
        contrary: Arc<[EvidenceCitation]>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        ambiguities: Vec<AmbiguityFlag>,
        derivation: CauseDerivation,
    ) -> Result<Self, DebuggerError> {
        Self::new_indexed(
            None,
            category,
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            ambiguities,
            derivation,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "complete causal evidence remains explicit")]
    fn new_indexed(
        manifest: Option<&TraceSelectionManifest>,
        category: FailureCategory,
        statement: DiagnosticText,
        support: Arc<[EvidenceCitation]>,
        contrary: Arc<[EvidenceCitation]>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        ambiguities: Vec<AmbiguityFlag>,
        derivation: CauseDerivation,
    ) -> Result<Self, DebuggerError> {
        if let Some(manifest) = manifest {
            validate_citations(&support, manifest)?;
        } else if support.is_empty() || support.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(report_error("support citations must be nonempty and canonical"));
        }
        if contrary.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(report_error("contrary citations must be strictly ordered and unique"));
        }
        if let Some(manifest) = manifest {
            contrary.iter().try_for_each(|citation| citation.validate_against(manifest))?;
        }
        if support.iter().any(|citation| contrary.binary_search(citation).is_ok()) {
            return Err(report_error("one citation cannot be both supporting and contrary"));
        }
        alternatives.validate(category)?;
        if ambiguities.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(report_error("ambiguity flags must be strictly ordered and unique"));
        }
        if confidence.basis().ambiguity_count()
            != u32::try_from(ambiguities.len()).unwrap_or(u32::MAX)
        {
            return Err(report_error("confidence basis does not match retained ambiguities"));
        }
        let id = derive_cause_id(
            category,
            &statement,
            &support,
            &contrary,
            &alternatives,
            confidence,
            &ambiguities,
            derivation,
        )?;
        Ok(Self {
            id,
            category,
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            ambiguities,
            derivation,
        })
    }

    /// Returns the stable cause identity.
    #[must_use]
    pub const fn id(&self) -> CauseId {
        self.id
    }
    /// Returns the closed taxonomy category.
    #[must_use]
    pub const fn category(&self) -> FailureCategory {
        self.category
    }
    /// Borrows the checked statement.
    #[must_use]
    pub const fn statement(&self) -> &DiagnosticText {
        &self.statement
    }
    /// Borrows supporting citations.
    #[must_use]
    pub fn support(&self) -> &[EvidenceCitation] {
        &self.support
    }
    /// Borrows contrary citations, including an explicit empty inventory.
    #[must_use]
    pub fn contrary(&self) -> &[EvidenceCitation] {
        &self.contrary
    }
    /// Borrows distinct alternatives.
    #[must_use]
    pub const fn alternatives(&self) -> &AlternativeCauses {
        &self.alternatives
    }
    /// Returns bounded evidence strength.
    #[must_use]
    pub const fn confidence(&self) -> ConfidenceMillionths {
        self.confidence
    }
    /// Borrows explicit ambiguity flags.
    #[must_use]
    pub fn ambiguities(&self) -> &[AmbiguityFlag] {
        &self.ambiguities
    }
    /// Returns the checked derivation boundary.
    #[must_use]
    pub const fn derivation(&self) -> CauseDerivation {
        self.derivation
    }
}

/// Typed reason an inert proposal cannot become an observation or cause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum UnsupportedReason {
    /// No selected evidence supports the conclusion.
    MissingSupport,
    /// A cited event or artifact is outside the frozen manifest.
    InvalidCitation,
    /// The proposal claims mutation, acceptance, evaluation, or promotion authority.
    AuthorityClaim,
    /// The proposal conflicts with deterministic binding or outcome facts.
    DeterministicConflict,
    /// The proposal exceeded a frozen resource limit.
    BoundExceeded,
}

/// Digest-only retention of an unsupported conclusion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnsupportedConclusion {
    proposal_digest: Sha256Digest,
    reason: UnsupportedReason,
}

impl UnsupportedConclusion {
    /// Retains only a safe digest and typed reason, never an actionable payload.
    #[must_use]
    pub const fn new(proposal_digest: Sha256Digest, reason: UnsupportedReason) -> Self {
        Self { proposal_digest, reason }
    }
    /// Returns the rejected proposal digest.
    #[must_use]
    pub const fn proposal_digest(self) -> Sha256Digest {
        self.proposal_digest
    }
    /// Returns the rejection reason.
    #[must_use]
    pub const fn reason(self) -> UnsupportedReason {
        self.reason
    }
}

#[allow(clippy::too_many_arguments, reason = "canonical cause fields mirror the checked value")]
fn derive_cause_id(
    category: FailureCategory,
    statement: &DiagnosticText,
    support: &[EvidenceCitation],
    contrary: &[EvidenceCitation],
    alternatives: &AlternativeCauses,
    confidence: ConfidenceMillionths,
    ambiguities: &[AmbiguityFlag],
    derivation: CauseDerivation,
) -> Result<CauseId, DebuggerError> {
    let mut hash = Sha256::new();
    hash.update(CAUSE_ID_DOMAIN);
    hash.update(b"peritus-e2-root-cause-v1\0");
    hash.update(category.tag().to_be_bytes());
    encode_blob(&mut hash, statement.as_str().as_bytes());
    encode_citations(&mut hash, support);
    encode_citations(&mut hash, contrary);
    match alternatives {
        AlternativeCauses::NoneKnown => hash.update([0]),
        AlternativeCauses::Categories(categories) => {
            hash.update([1]);
            encode_len(&mut hash, categories.len());
            for category in categories {
                hash.update(category.tag().to_be_bytes());
            }
        }
    }
    hash.update(confidence.value().to_be_bytes());
    let basis = confidence.basis();
    for value in [
        basis.support_count(),
        basis.contrary_count(),
        basis.ambiguity_count(),
        basis.recurrence_count(),
        basis.maximum_causal_distance(),
    ] {
        hash.update(value.to_be_bytes());
    }
    encode_len(&mut hash, ambiguities.len());
    for ambiguity in ambiguities {
        hash.update([ambiguity_tag(*ambiguity)]);
    }
    match derivation {
        CauseDerivation::Deterministic => hash.update([1]),
        CauseDerivation::ValidatedModel(id) => {
            hash.update([2]);
            hash.update(id.as_bytes());
        }
    }
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    CauseId::new(identity)
}

fn encode_citations(hash: &mut Sha256, citations: &[EvidenceCitation]) {
    encode_len(hash, citations.len());
    for citation in citations {
        citation.encode_to(|part| hash.update(part));
    }
}

fn encode_len(hash: &mut Sha256, length: usize) {
    hash.update(u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

fn encode_blob(hash: &mut Sha256, value: &[u8]) {
    encode_len(hash, value.len());
    hash.update(value);
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "canonical tag encoding is shared by sibling private modules only"
)]
pub(crate) const fn ambiguity_tag(value: AmbiguityFlag) -> u8 {
    match value {
        AmbiguityFlag::MissingCausalPredecessor => 1,
        AmbiguityFlag::ClockDisagreement => 2,
        AmbiguityFlag::MultiplePlausibleCauses => 3,
        AmbiguityFlag::IncompleteSpan => 4,
        AmbiguityFlag::CrossRevisionAttribution => 5,
    }
}

fn report_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Report,
        DebuggerOperation::AnalyzeCauses,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
