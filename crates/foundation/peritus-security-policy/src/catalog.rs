//! Closed authoritative H0 requirement and evidence identities.

#[doc = " Authoritative security requirement identifier."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityRequirement {
    #[doc = " R-SEC-001: model output proposes actions but never grants authority or mutates state."]
    RSec001,
    #[doc = " R-SEC-002: effects require scoped, expiring, actor-bound capabilities."]
    RSec002,
    #[doc = " R-SEC-003: path authorization resists traversal, races, aliases, and mount tricks."]
    RSec003,
    #[doc = " R-SEC-004: native backends enforce sandbox controls rather than prompts alone."]
    RSec004,
    #[doc = " R-SEC-005: security-sensitive metadata is protected unless explicitly authorized."]
    RSec005,
    #[doc = " R-SEC-006: provenance cannot silently change authority precedence."]
    RSec006,
    #[doc = " R-SEC-007: dependencies, plugins, artifacts, SBOMs, and signatures are auditable."]
    RSec007,
}
impl SecurityRequirement {
    #[doc = " Complete canonical R-SEC-001 through R-SEC-007 sequence."]
    pub const ALL: [Self; 7] = [
        Self::RSec001,
        Self::RSec002,
        Self::RSec003,
        Self::RSec004,
        Self::RSec005,
        Self::RSec006,
        Self::RSec007,
    ];
    #[doc = " Returns the literal architecture requirement label."]
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
#[doc = " Authoritative numbered acceptance criterion owned by the H0 campaign."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AcceptanceCriterion {
    #[doc = " Criterion 9: malicious-repository attacks are covered."]
    Criterion9,
    #[doc = " Criterion 10: tier-one sandboxes pass conformance and escape review."]
    Criterion10,
    #[doc = " Criterion 11: writer, reviewer, and fixer authority is isolated."]
    Criterion11,
    #[doc = " Criterion 12: candidate mutation invalidates prior evidence."]
    Criterion12,
    #[doc = " Criterion 17: evolution candidates cannot reach protected evaluation authority."]
    Criterion17,
    #[doc = " Criterion 18: promotion and rollback preserve immutable gated histories."]
    Criterion18,
    #[doc = " Criterion 19: observability is cited, classified, and secret-redacted."]
    Criterion19,
    #[doc = " Criterion 24: artifacts are reproducible, signed, documented, and security-reviewed."]
    Criterion24,
    #[doc = " Criterion 25: no quarantined failures, blockers, undocumented unsafe, or placeholders."]
    Criterion25,
}
impl AcceptanceCriterion {
    #[doc = " Complete canonical sequence of criteria assigned to H0."]
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
    #[doc = " Returns the architecture criterion number."]
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
#[doc = " Required reviewed inventory."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InventoryKind {
    #[doc = " Versioned threat inventory with assets, boundaries, threats, and owners."]
    Threats,
    #[doc = " Requirement-to-control and control-to-probe traceability matrix."]
    Controls,
    #[doc = " Complete workspace unsafe-code and safety-justification inventory."]
    UnsafeCode,
    #[doc = " Trusted computing base, trusted construct, toolchain, and native-backend inventory."]
    TrustedComputingBase,
}
impl InventoryKind {
    #[doc = " Complete canonical inventory sequence."]
    pub const ALL: [Self; 4] =
        [Self::Threats, Self::Controls, Self::UnsafeCode, Self::TrustedComputingBase];
}
#[doc = " Required role in the canonical H0 evidence manifest."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EvidenceArtifactKind {
    #[doc = " Exact campaign plan and closed probe catalog."]
    CampaignPlan,
    #[doc = " Native probe observations and direct output digests."]
    NativeProbeResults,
    #[doc = " Per-case resource accounting."]
    ResourceAccounting,
    #[doc = " Per-case cancellation and cleanup ledger."]
    CleanupLedger,
    #[doc = " Reviewed threat and control inventories."]
    ThreatControlInventory,
    #[doc = " Reviewed unsafe-code and TCB inventories."]
    UnsafeTcbInventory,
    #[doc = " Independent external security-review report."]
    ExternalReviewReport,
    #[doc = " Complete finding and remediation/retest register."]
    FindingRegister,
    #[doc = " Reproducible artifact, SBOM, provenance, license, and signature attestation."]
    SupplyChainAttestation,
    #[doc = " Exact release artifact manifest named by the integrated candidate."]
    ReleaseManifest,
}
impl EvidenceArtifactKind {
    #[doc = " Complete canonical evidence role sequence."]
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
