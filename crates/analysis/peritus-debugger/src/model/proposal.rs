//! Closed JSON proposal decoding and manifest-contained validation.

use peritus_harness::domain::ComponentKind;
use peritus_model_protocol::CanonicalJson;
use peritus_types::{EventId, Sha256Digest};
use serde::Deserialize;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerLimit, DebuggerLimits, DebuggerOperation,
    DebuggerRecovery, DiagnosticText, EvidenceCitation, SelectionManifestId, SubjectId,
    TraceSelectionManifest, validate_citations,
};

/// One strictly evidence-linked model finding, still inert and non-authoritative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFinding {
    statement: DiagnosticText,
    citations: Vec<EvidenceCitation>,
}

impl ModelFinding {
    /// Validated statement.
    #[must_use]
    pub const fn statement(&self) -> &DiagnosticText {
        &self.statement
    }
    /// Manifest-contained supporting citations.
    #[must_use]
    pub fn citations(&self) -> &[EvidenceCitation] {
        &self.citations
    }
}

/// One strictly evidence-linked non-authoritative recommendation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRecommendation {
    statement: DiagnosticText,
    citations: Vec<EvidenceCitation>,
    affected_components: Vec<ComponentKind>,
}

impl ModelRecommendation {
    /// Validated recommendation text.
    #[must_use]
    pub const fn statement(&self) -> &DiagnosticText {
        &self.statement
    }
    /// Manifest-contained supporting citations.
    #[must_use]
    pub fn citations(&self) -> &[EvidenceCitation] {
        &self.citations
    }
    /// Canonical affected E1 component classes.
    #[must_use]
    pub fn affected_components(&self) -> &[ComponentKind] {
        &self.affected_components
    }
}

/// Structured proposal proven bound to the exact selection and deterministic analysis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedModelProposal {
    manifest_id: SelectionManifestId,
    manifest_digest: Sha256Digest,
    deterministic_digest: Sha256Digest,
    findings: Vec<ModelFinding>,
    recommendations: Vec<ModelRecommendation>,
    canonical_bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl ValidatedModelProposal {
    /// Parses and validates the only closed optional-model result schema.
    ///
    /// # Errors
    /// Rejects unknown/missing fields, binding drift, invalid text, noncanonical collections,
    /// citations outside the manifest, or job-limit excess.
    pub fn validate(
        value: &CanonicalJson,
        manifest: &TraceSelectionManifest,
        deterministic_digest: Sha256Digest,
        limits: DebuggerLimits,
    ) -> Result<Self, DebuggerError> {
        let wire: ProposalWire = serde_json::from_slice(value.canonical_bytes())
            .map_err(|_| rejected("structured proposal does not match the closed E2 schema"))?;
        if wire.schema_version != 1
            || parse_id::<16>(&wire.manifest_id)? != *manifest.id().as_bytes()
            || parse_id::<32>(&wire.manifest_digest)? != *manifest.digest().as_bytes()
            || parse_id::<32>(&wire.deterministic_digest)? != *deterministic_digest.as_bytes()
        {
            return Err(rejected(
                "structured proposal binding differs from selection or deterministic analysis",
            ));
        }
        limits.check(
            DebuggerLimit::ModelOutputBytes,
            value.canonical_bytes().len(),
            DebuggerOperation::RunModelAnalysis,
        )?;
        let claim_count = wire
            .findings
            .len()
            .checked_add(wire.recommendations.len())
            .ok_or_else(|| budget("model proposal claim count overflowed"))?;
        limits.check(DebuggerLimit::Claims, claim_count, DebuggerOperation::RunModelAnalysis)?;
        let findings = wire
            .findings
            .into_iter()
            .map(|finding| {
                Ok(ModelFinding {
                    statement: DiagnosticText::new(finding.statement)?,
                    citations: citations(finding.citations, manifest)?,
                })
            })
            .collect::<Result<Vec<_>, DebuggerError>>()?;
        if findings.windows(2).any(|pair| pair[0].statement >= pair[1].statement) {
            return Err(rejected("model findings are not in canonical statement order"));
        }
        let recommendations = wire
            .recommendations
            .into_iter()
            .map(|recommendation| {
                let affected_components = recommendation
                    .affected_component_tags
                    .into_iter()
                    .map(component_kind)
                    .collect::<Result<Vec<_>, DebuggerError>>()?;
                if affected_components.is_empty()
                    || affected_components.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err(rejected(
                        "recommendation component classes are not nonempty and canonical",
                    ));
                }
                Ok(ModelRecommendation {
                    statement: DiagnosticText::new(recommendation.statement)?,
                    citations: citations(recommendation.citations, manifest)?,
                    affected_components,
                })
            })
            .collect::<Result<Vec<_>, DebuggerError>>()?;
        if recommendations.windows(2).any(|pair| pair[0].statement >= pair[1].statement) {
            return Err(rejected("model recommendations are not in canonical statement order"));
        }
        let canonical_bytes = value.canonical_bytes().to_vec();
        let mut identity = b"peritus.debugger.validated-model-proposal.v1\0".to_vec();
        identity.extend_from_slice(&canonical_bytes);
        let digest = peritus_codec::sha256(&identity);
        Ok(Self {
            manifest_id: manifest.id(),
            manifest_digest: manifest.digest(),
            deterministic_digest,
            findings,
            recommendations,
            canonical_bytes,
            digest,
        })
    }

    /// Exact bound manifest identity.
    #[must_use]
    pub const fn manifest_id(&self) -> SelectionManifestId {
        self.manifest_id
    }
    /// Exact bound manifest digest.
    #[must_use]
    pub const fn manifest_digest(&self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Exact deterministic-analysis digest.
    #[must_use]
    pub const fn deterministic_digest(&self) -> Sha256Digest {
        self.deterministic_digest
    }
    /// Validated model findings.
    #[must_use]
    pub fn findings(&self) -> &[ModelFinding] {
        &self.findings
    }
    /// Validated non-authoritative recommendations.
    #[must_use]
    pub fn recommendations(&self) -> &[ModelRecommendation] {
        &self.recommendations
    }
    /// Canonical structured item bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Domain-separated proposal digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalWire {
    schema_version: u16,
    manifest_id: String,
    manifest_digest: String,
    deterministic_digest: String,
    findings: Vec<FindingWire>,
    recommendations: Vec<RecommendationWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindingWire {
    statement: String,
    citations: Vec<CitationWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecommendationWire {
    statement: String,
    citations: Vec<CitationWire>,
    affected_component_tags: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CitationWire {
    subject_id: String,
    event_id: String,
    journal_position: u64,
    frame_digest: String,
}

fn citations(
    values: Vec<CitationWire>,
    manifest: &TraceSelectionManifest,
) -> Result<Vec<EvidenceCitation>, DebuggerError> {
    let citations = values
        .into_iter()
        .map(|value| {
            EvidenceCitation::new(
                manifest,
                SubjectId::new(parse_id(&value.subject_id)?)?,
                EventId::new(parse_id(&value.event_id)?)
                    .map_err(|_| rejected("proposal citation event identity is invalid"))?,
                value.journal_position,
                Sha256Digest::new(parse_id(&value.frame_digest)?),
                None,
            )
        })
        .collect::<Result<Vec<_>, DebuggerError>>()?;
    validate_citations(&citations, manifest)?;
    Ok(citations)
}

fn component_kind(tag: u8) -> Result<ComponentKind, DebuggerError> {
    ComponentKind::ALL
        .into_iter()
        .find(|kind| kind.tag() == tag)
        .ok_or_else(|| rejected("proposal names an unknown component class"))
}

fn parse_id<const N: usize>(value: &str) -> Result<[u8; N], DebuggerError> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(rejected("proposal contains a malformed hexadecimal binding"));
    }
    let mut bytes = [0_u8; N];
    for (index, output) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *output = (nibble(value.as_bytes()[offset])? << 4) | nibble(value.as_bytes()[offset + 1])?;
    }
    Ok(bytes)
}

fn nibble(value: u8) -> Result<u8, DebuggerError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(rejected("proposal hexadecimal bindings must be lowercase")),
    }
}

fn rejected(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::ModelRejected,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::None,
        detail,
    )
}

fn budget(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::None,
        detail,
    )
}
