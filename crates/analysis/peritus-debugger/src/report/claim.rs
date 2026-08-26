//! Claim kinds whose constructors encode their evidence obligations.

use crate::{
    AlternativeCauses, ClaimId, ConfidenceMillionths, DebuggerError, DebuggerErrorKind,
    DebuggerOperation, DebuggerRecovery, DiagnosticText, EvidenceCitation, FailureCategory,
    UnsupportedConclusion,
};
use peritus_harness::domain::ComponentKind;

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
    /// Rejects missing/noncanonical support or inferential/recommendation language.
    pub fn observation(
        statement: DiagnosticText,
        support: Vec<EvidenceCitation>,
    ) -> Result<Self, DebuggerError> {
        validate_nonempty_canonical(
            &support,
            "observation support must be canonical and nonempty",
        )?;
        let lower = statement.as_str().to_ascii_lowercase();
        if [" might ", " likely ", " probably ", " recommend", " should "]
            .iter()
            .any(|marker| lower.contains(marker))
        {
            return Err(claim_error(
                "observation statement contains an unsupported language marker",
            ));
        }
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
        let mut bytes = Vec::new();
        super::canonical::encode_claim_content(&mut bytes, &content);
        Ok(Self { id: ClaimId::derive(CLAIM_ID_DOMAIN, &bytes)?, content })
    }
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
