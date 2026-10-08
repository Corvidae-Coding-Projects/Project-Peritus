//! Claim kinds whose constructors encode their evidence obligations.

use crate::{
    AlternativeCauses, ClaimId, ConfidenceMillionths, DebuggerError, DebuggerErrorKind,
    DebuggerOperation, DebuggerRecovery, DiagnosticText, EvidenceCitation, FailureCategory,
    UnsupportedConclusion,
};
use peritus_harness::domain::ComponentKind;
use sha2::{Digest, Sha256};

const CLAIM_ID_DOMAIN: &[u8] = b"peritus-e2-report-claim-id-v1\0";

/// Semantic status of one immutable report statement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ClaimKind {
    /// Direct statement of selected evidence.
    Observation,
    /// Evidence-linked interpretation with uncertainty.
    Inference,
    /// Non-authoritative proposed follow-up linked to a supported parent.
    Recommendation,
    /// Digest-only retention of a rejected unsupported proposal.
    UnsupportedConclusion,
}

/// Kind-specific immutable claim content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ClaimContent {
    Observation {
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
    },
    Inference {
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
        contrary: Vec<EvidenceCitation>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        category: FailureCategory,
    },
    Recommendation {
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
        parent: ClaimId,
        affected_components: Vec<ComponentKind>,
    },
    Unsupported(UnsupportedConclusion),
}

/// One immutable typed report claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportClaim {
    pub(super) id: ClaimId,
    pub(super) content: ClaimContent,
}

impl ReportClaim {
    /// Creates a direct observation with nonempty canonical support.
    ///
    /// # Errors
    ///
    /// Rejects missing or noncanonical supporting provenance.
    pub fn observation(
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
    ) -> Result<Self, DebuggerError> {
        validate_nonempty_canonical(
            &support,
            "observation support must be canonical and nonempty",
        )?;
        Self::derive(ClaimContent::Observation { statement, support })
    }

    /// Creates an inference with complete explicit uncertainty inventory.
    ///
    /// # Errors
    ///
    /// Rejects missing support, noncanonical contrary evidence, or support/contrary overlap.
    #[allow(clippy::too_many_arguments, reason = "inference uncertainty remains explicit")]
    pub fn inference(
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
        contrary: Vec<EvidenceCitation>,
        alternatives: AlternativeCauses,
        confidence: ConfidenceMillionths,
        category: FailureCategory,
    ) -> Result<Self, DebuggerError> {
        validate_nonempty_canonical(&support, "inference support must be canonical and nonempty")?;
        validate_optional_canonical(&contrary, "inference contrary evidence must be canonical")?;
        if support.iter().any(|citation| contrary.binary_search(citation).is_ok()) {
            return Err(claim_error("inference support and contrary evidence overlap"));
        }
        alternatives.validate(category)?;
        Self::derive(ClaimContent::Inference {
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            category,
        })
    }

    /// Creates a non-authoritative recommendation linked to an observation or inference parent.
    ///
    /// Parent kind and existence are checked against the complete report.
    ///
    /// # Errors
    ///
    /// Rejects missing or noncanonical support.
    pub fn recommendation(
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
        parent: ClaimId,
        affected_components: Vec<ComponentKind>,
    ) -> Result<Self, DebuggerError> {
        validate_nonempty_canonical(
            &support,
            "recommendation support must be canonical and nonempty",
        )?;
        if affected_components.is_empty()
            || affected_components.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(claim_error(
                "recommendation component classes must be nonempty and canonical",
            ));
        }
        Self::derive(ClaimContent::Recommendation {
            statement,
            support,
            parent,
            affected_components,
        })
    }

    /// Retains one digest-only unsupported conclusion.
    ///
    /// # Errors
    ///
    /// Returns only if the derived claim identity hits the reserved zero projection.
    pub fn unsupported(value: UnsupportedConclusion) -> Result<Self, DebuggerError> {
        Self::derive(ClaimContent::Unsupported(value))
    }

    /// Returns the stable claim identity.
    #[must_use]
    pub const fn id(&self) -> ClaimId {
        self.id
    }
    /// Returns immutable semantic claim kind.
    #[must_use]
    pub const fn kind(&self) -> ClaimKind {
        match self.content {
            ClaimContent::Observation { .. } => ClaimKind::Observation,
            ClaimContent::Inference { .. } => ClaimKind::Inference,
            ClaimContent::Recommendation { .. } => ClaimKind::Recommendation,
            ClaimContent::Unsupported(_) => ClaimKind::UnsupportedConclusion,
        }
    }
    /// Returns statement text for supported kinds.
    #[must_use]
    pub const fn statement(&self) -> Option<&DiagnosticText> {
        match &self.content {
            ClaimContent::Observation { statement, .. }
            | ClaimContent::Inference { statement, .. }
            | ClaimContent::Recommendation { statement, .. } => Some(statement),
            ClaimContent::Unsupported(_) => None,
        }
    }
    /// Borrows supporting citations; unsupported conclusions have none.
    #[must_use]
    pub fn support(&self) -> &[EvidenceCitation] {
        match &self.content {
            ClaimContent::Observation { support, .. }
            | ClaimContent::Inference { support, .. }
            | ClaimContent::Recommendation { support, .. } => support,
            ClaimContent::Unsupported(_) => &[],
        }
    }
    /// Borrows contrary evidence, present only for inferences.
    #[must_use]
    pub fn contrary(&self) -> &[EvidenceCitation] {
        match &self.content {
            ClaimContent::Inference { contrary, .. } => contrary,
            _ => &[],
        }
    }
    /// Returns a recommendation parent.
    #[must_use]
    pub const fn parent(&self) -> Option<ClaimId> {
        match self.content {
            ClaimContent::Recommendation { parent, .. } => Some(parent),
            _ => None,
        }
    }
    /// Borrows affected E1 component classes for a recommendation.
    #[must_use]
    pub fn affected_components(&self) -> &[ComponentKind] {
        match &self.content {
            ClaimContent::Recommendation { affected_components, .. } => affected_components,
            _ => &[],
        }
    }
    /// Returns inference confidence.
    #[must_use]
    pub const fn confidence(&self) -> Option<ConfidenceMillionths> {
        match self.content {
            ClaimContent::Inference { confidence, .. } => Some(confidence),
            _ => None,
        }
    }
    /// Returns a digest-only unsupported conclusion.
    #[must_use]
    pub const fn unsupported_conclusion(&self) -> Option<UnsupportedConclusion> {
        match self.content {
            ClaimContent::Unsupported(value) => Some(value),
            _ => None,
        }
    }

    fn derive(content: ClaimContent) -> Result<Self, DebuggerError> {
        let id = derive_claim_id(&content)?;
        Ok(Self { id, content })
    }
}

fn derive_claim_id(content: &ClaimContent) -> Result<ClaimId, DebuggerError> {
    let mut hash = Sha256::new();
    hash.update(CLAIM_ID_DOMAIN);
    match content {
        ClaimContent::Observation { statement, support } => {
            hash.update([1]);
            encode_text(&mut hash, statement);
            encode_citations(&mut hash, support);
        }
        ClaimContent::Inference {
            statement,
            support,
            contrary,
            alternatives,
            confidence,
            category,
        } => {
            hash.update([2]);
            encode_text(&mut hash, statement);
            encode_citations(&mut hash, support);
            encode_citations(&mut hash, contrary);
            encode_alternatives(&mut hash, alternatives);
            hash.update(confidence.value().to_be_bytes());
            encode_confidence_basis(&mut hash, confidence.basis());
            hash.update(category.tag().to_be_bytes());
        }
        ClaimContent::Recommendation { statement, support, parent, affected_components } => {
            hash.update([3]);
            encode_text(&mut hash, statement);
            encode_citations(&mut hash, support);
            hash.update(parent.as_bytes());
            encode_len(&mut hash, affected_components.len());
            for component in affected_components {
                hash.update([component.tag()]);
            }
        }
        ClaimContent::Unsupported(value) => {
            hash.update([4]);
            hash.update(value.proposal_digest().as_bytes());
            hash.update([value.reason() as u8]);
        }
    }
    let digest: [u8; 32] = hash.finalize().into();
    let mut identity = [0_u8; 16];
    identity.copy_from_slice(&digest[..16]);
    ClaimId::new(identity)
}

fn encode_text(hash: &mut Sha256, text: &DiagnosticText) {
    encode_blob(hash, text.as_str().as_bytes());
}

fn encode_citations(hash: &mut Sha256, values: &[EvidenceCitation]) {
    encode_len(hash, values.len());
    for value in values {
        value.encode_to(|part| hash.update(part));
    }
}

fn encode_alternatives(hash: &mut Sha256, value: &AlternativeCauses) {
    match value {
        AlternativeCauses::NoneKnown => hash.update([0]),
        AlternativeCauses::Categories(values) => {
            hash.update([1]);
            encode_len(hash, values.len());
            for value in values {
                hash.update(value.tag().to_be_bytes());
            }
        }
    }
}

fn encode_confidence_basis(hash: &mut Sha256, value: crate::ConfidenceBasis) {
    for count in [
        value.support_count(),
        value.contrary_count(),
        value.ambiguity_count(),
        value.recurrence_count(),
        value.maximum_causal_distance(),
    ] {
        hash.update(count.to_be_bytes());
    }
}

fn encode_len(hash: &mut Sha256, length: usize) {
    hash.update(u64::try_from(length).unwrap_or(u64::MAX).to_be_bytes());
}

fn encode_blob(hash: &mut Sha256, value: &[u8]) {
    encode_len(hash, value.len());
    hash.update(value);
}

fn validate_nonempty_canonical(
    values: &[EvidenceCitation],
    detail: &'static str,
) -> Result<(), DebuggerError> {
    if values.is_empty() {
        return Err(claim_error(detail));
    }
    validate_optional_canonical(values, detail)
}

fn validate_optional_canonical(
    values: &[EvidenceCitation],
    detail: &'static str,
) -> Result<(), DebuggerError> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) { Err(claim_error(detail)) } else { Ok(()) }
}

fn claim_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Report,
        DebuggerOperation::ValidateReport,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
