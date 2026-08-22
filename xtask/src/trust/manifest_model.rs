use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) entries: Vec<TrustEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustEntry {
    pub(super) id: String,
    pub(super) symbol: String,
    pub(super) owning_crate: String,
    pub(super) source_file: String,
    pub(super) source_line: u64,
    pub(super) construct_kind: String,
    pub(super) upstream: String,
    pub(super) upstream_version: String,
    pub(super) assumed_contract: String,
    pub(super) threat_if_false: String,
    pub(super) evidence: Vec<BoundaryEvidence>,
    pub(super) live_issue: String,
    pub(super) owner: String,
    pub(super) reviewer: String,
    pub(super) review_date: String,
    pub(super) expiry_date: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExclusionsDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) entries: Vec<ExclusionEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExclusionEntry {
    pub(super) id: String,
    pub(super) symbol: String,
    pub(super) owning_crate: String,
    pub(super) source_file: String,
    pub(super) source_line: u64,
    pub(super) verification_class: VerificationClass,
    pub(super) unsupported_feature: String,
    pub(super) risk: String,
    pub(super) evidence: Vec<BoundaryEvidence>,
    pub(super) live_issue: String,
    pub(super) owner: String,
    pub(super) reviewer: String,
    pub(super) review_date: String,
    pub(super) upstream_tracking: String,
    pub(super) revisit_plan: String,
    pub(super) revisit_by: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub(super) enum VerificationClass {
    H,
    T,
}

impl VerificationClass {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::H => "H",
            Self::T => "T",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BoundaryEvidence {
    pub(super) kind: BoundaryEvidenceKind,
    pub(super) source_file: String,
    pub(super) symbol: String,
    pub(super) command: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BoundaryEvidenceKind {
    RefinementTest,
    ConformanceTest,
    FaultInjectionTest,
    ModelCheck,
}

impl BoundaryEvidenceKind {
    pub(super) const fn is_refinement_or_conformance(self) -> bool {
        matches!(self, Self::RefinementTest | Self::ConformanceTest)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObligationsDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) entries: Vec<ObligationEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObligationEntry {
    pub(super) id: String,
    pub(super) kind: ObligationKind,
    pub(super) statement: String,
    pub(super) owning_crate: String,
    pub(super) source_file: String,
    pub(super) symbol: String,
    pub(super) status: ObligationStatus,
    pub(super) dependencies: Vec<String>,
    pub(super) live_issue: String,
    pub(super) owner: String,
    pub(super) evidence: Vec<ProofEvidence>,
    pub(super) reviewer: Option<String>,
    pub(super) review_date: Option<String>,
    pub(super) exclusion_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ObligationKind {
    Invariant,
    Contract,
    Refinement,
    Termination,
    Liveness,
}

impl ObligationKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Contract => "contract",
            Self::Refinement => "refinement",
            Self::Termination => "termination",
            Self::Liveness => "liveness",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ObligationStatus {
    Open,
    InProgress,
    Discharged,
    Excluded,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofEvidence {
    pub(super) kind: ProofEvidenceKind,
    pub(super) source_file: String,
    pub(super) symbol: String,
    pub(super) command: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactDocument {
    pub(super) schema: String,
    pub(super) schema_version: u64,
    pub(super) baseline: String,
    pub(super) hash_algorithm: String,
    pub(super) sources: Vec<ProofImpactSource>,
    pub(super) changes: Vec<ProofImpactChange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactSource {
    pub(super) source_file: String,
    pub(super) sha256: String,
    pub(super) affected_packages: Vec<ProofImpactPackage>,
    pub(super) change_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactPackage {
    pub(super) package: String,
    pub(super) verification_class: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactChange {
    pub(super) id: String,
    pub(super) status: ProofImpactStatus,
    pub(super) change_kinds: Vec<ProofImpactKind>,
    pub(super) source_changes: Vec<ProofSourceChange>,
    pub(super) rationale: String,
    pub(super) impact: String,
    pub(super) evidence: Vec<ProofImpactEvidence>,
    pub(super) owner: String,
    pub(super) reviewer: String,
    pub(super) review_date: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofSourceChange {
    pub(super) source_file: String,
    pub(super) previous: Option<ProofImpactSnapshot>,
    pub(super) current: Option<ProofImpactSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactSnapshot {
    pub(super) sha256: String,
    pub(super) affected_packages: Vec<ProofImpactPackage>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProofImpactStatus {
    Approved,
    Revoked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProofImpactKind {
    Specification,
    Precondition,
    Postcondition,
    Proof,
    Executable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProofImpactEvidence {
    pub(super) kind: ProofImpactEvidenceKind,
    pub(super) owning_crate: String,
    pub(super) command: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProofImpactEvidenceKind {
    OrdinaryTest,
    VerusVerify,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProofEvidenceKind {
    VerusProof,
    ModelCheck,
    RefinementTest,
    PropertyTest,
}

impl ProofEvidenceKind {
    pub(super) const fn is_formal(self) -> bool {
        matches!(self, Self::VerusProof | Self::ModelCheck)
    }
}
