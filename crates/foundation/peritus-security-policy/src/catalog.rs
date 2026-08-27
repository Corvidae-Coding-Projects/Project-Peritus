//! Closed authoritative H0 requirement and evidence identities.

use vstd::prelude::*;

verus! {

/// Authoritative security requirement identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityRequirement {
    /// R-SEC-001: model output proposes actions but never grants authority or mutates state.
    RSec001,
    /// R-SEC-002: effects require scoped, expiring, actor-bound capabilities.
    RSec002,
    /// R-SEC-003: path authorization resists traversal, races, aliases, and mount tricks.
    RSec003,
    /// R-SEC-004: native backends enforce sandbox controls rather than prompts alone.
    RSec004,
    /// R-SEC-005: security-sensitive metadata is protected unless explicitly authorized.
    RSec005,
    /// R-SEC-006: provenance cannot silently change authority precedence.
    RSec006,
    /// R-SEC-007: dependencies, plugins, artifacts, SBOMs, and signatures are auditable.
    RSec007,
}

impl SecurityRequirement {
    /// Complete canonical R-SEC-001 through R-SEC-007 sequence.
    pub const ALL: [Self; 7] = [
        Self::RSec001,
        Self::RSec002,
        Self::RSec003,
        Self::RSec004,
        Self::RSec005,
        Self::RSec006,
        Self::RSec007,
    ];

    /// Returns the literal architecture requirement label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RSec001 => "R-SEC-001",
            Self::RSec002 => "R-SEC-002",
            Self::RSec003 => "R-SEC-003",
            Self::RSec004 => "R-SEC-004",
            Self::RSec005 => "R-SEC-005",
            Self::RSec006 => "R-SEC-006",
            Self::RSec007 => "R-SEC-007",
        }
    }
}

/// Authoritative numbered acceptance criterion owned by the H0 campaign.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AcceptanceCriterion {
    /// Criterion 9: malicious-repository attacks are covered.
    Criterion9,
    /// Criterion 10: tier-one sandboxes pass conformance and escape review.
    Criterion10,
    /// Criterion 11: writer, reviewer, and fixer authority is isolated.
    Criterion11,
    /// Criterion 12: candidate mutation invalidates prior evidence.
    Criterion12,
    /// Criterion 17: evolution candidates cannot reach protected evaluation authority.
    Criterion17,
    /// Criterion 18: promotion and rollback preserve immutable gated histories.
    Criterion18,
    /// Criterion 19: observability is cited, classified, and secret-redacted.
    Criterion19,
    /// Criterion 24: artifacts are reproducible, signed, documented, and security-reviewed.
    Criterion24,
    /// Criterion 25: no quarantined failures, blockers, undocumented unsafe, or placeholders.
    Criterion25,
}

impl AcceptanceCriterion {
    /// Complete canonical sequence of criteria assigned to H0.
    pub const ALL: [Self; 9] = [
        Self::Criterion9,
        Self::Criterion10,
        Self::Criterion11,
        Self::Criterion12,
        Self::Criterion17,
        Self::Criterion18,
        Self::Criterion19,
        Self::Criterion24,
        Self::Criterion25,
    ];

    /// Returns the architecture criterion number.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Criterion9 => 9,
            Self::Criterion10 => 10,
            Self::Criterion11 => 11,
            Self::Criterion12 => 12,
            Self::Criterion17 => 17,
            Self::Criterion18 => 18,
            Self::Criterion19 => 19,
            Self::Criterion24 => 24,
            Self::Criterion25 => 25,
        }
    }
}

/// Required reviewed inventory.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InventoryKind {
    /// Versioned threat inventory with assets, boundaries, threats, and owners.
    Threats,
    /// Requirement-to-control and control-to-probe traceability matrix.
    Controls,
    /// Complete workspace unsafe-code and safety-justification inventory.
    UnsafeCode,
    /// Trusted computing base, trusted construct, toolchain, and native-backend inventory.
    TrustedComputingBase,
}

impl InventoryKind {
    /// Complete canonical inventory sequence.
    pub const ALL: [Self; 4] = [
        Self::Threats,
        Self::Controls,
        Self::UnsafeCode,
        Self::TrustedComputingBase,
    ];
}

/// Required role in the canonical H0 evidence manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceArtifactKind {
    /// Exact campaign plan and closed probe catalog.
    CampaignPlan,
    /// Native probe observations and direct output digests.
    NativeProbeResults,
    /// Per-case resource accounting.
    ResourceAccounting,
    /// Per-case cancellation and cleanup ledger.
    CleanupLedger,
    /// Reviewed threat and control inventories.
    ThreatControlInventory,
    /// Reviewed unsafe-code and TCB inventories.
    UnsafeTcbInventory,
    /// Independent external security-review report.
    ExternalReviewReport,
    /// Complete finding and remediation/retest register.
    FindingRegister,
    /// Reproducible artifact, SBOM, provenance, license, and signature attestation.
    SupplyChainAttestation,
    /// Exact release artifact manifest named by the integrated candidate.
    ReleaseManifest,
}

impl EvidenceArtifactKind {
    /// Complete canonical evidence role sequence.
    pub const ALL: [Self; 10] = [
        Self::CampaignPlan,
        Self::NativeProbeResults,
        Self::ResourceAccounting,
        Self::CleanupLedger,
        Self::ThreatControlInventory,
        Self::UnsafeTcbInventory,
        Self::ExternalReviewReport,
        Self::FindingRegister,
        Self::SupplyChainAttestation,
        Self::ReleaseManifest,
    ];
}

} // verus!
