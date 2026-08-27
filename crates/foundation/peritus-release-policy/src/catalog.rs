//! Closed production-acceptance and evidence catalogs.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use vstd::prelude::*;

verus! {

/// One stable production acceptance criterion from the architecture contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum AcceptanceCriterion {
    /// Clean tier-one formatting, lint, test, documentation, security, and end-to-end matrix.
    CleanTierOneSuite = 1,
    /// Clean locked Verus verification and verified release build.
    VerifiedWorkspaceBuild = 2,
    /// Complete proof-obligation inventory or narrowly approved exclusions.
    ProofObligationInventory = 3,
    /// No unapproved trusted construct outside the reviewed trust boundary.
    TrustedConstructAudit = 4,
    /// Privileged values and accepted states require verified transitions.
    PrivilegedConstruction = 5,
    /// Every illegal lifecycle edge is consistently rejected.
    IllegalLifecycleEdges = 6,
    /// Crash injection at every authoritative commit boundary recovers without false success.
    CrashRecovery = 7,
    /// Empty-database replay reproduces authoritative state and decisions byte-for-byte.
    DeterministicReplay = 8,
    /// Malicious-repository behavior is covered and rejected.
    MaliciousRepository = 9,
    /// Every tier-one sandbox passes conformance and independent escape review.
    NativeSandboxSecurity = 10,
    /// Writer, reviewer, and fixer roles retain mutation and self-approval isolation.
    RoleIsolation = 11,
    /// Candidate mutation invalidates prior gate and review evidence.
    EvidenceInvalidation = 12,
    /// Budget, retry, and timeout exhaustion fail closed with complete evidence.
    ExhaustionFailsClosed = 13,
    /// Daemon restart during every lifecycle phase leaves no orphaned authoritative work.
    DaemonLifecycleRecovery = 14,
    /// Provider contracts cover required interruption and malformed-stream behavior.
    ProviderContracts = 15,
    /// Historical schemas migrate, corrupt journals fail, and evidence exports remain portable.
    MigrationAndExport = 16,
    /// Evolution candidates cannot access sealed inputs, alter evaluators, or self-promote.
    EvolutionIsolation = 17,
    /// Promotion uses every immutable gate and rollback preserves both histories atomically.
    PromotionAndRollback = 18,
    /// Observability is cited, failure-classified, and secret-redacted.
    ObservabilityAndRedaction = 19,
    /// Load and soak runs meet the documented service-level objectives.
    LoadAndSoak = 20,
    /// Every public command and protocol method is documented and exercised end-to-end.
    PublicSurfaceDocumentation = 21,
    /// Architecture checks reject cycles, upward dependencies, god roots, and API leakage.
    ArchitectureIntegrity = 22,
    /// The final representative multi-language writer/reviewer/fixer campaign succeeds.
    RepresentativeCampaign = 23,
    /// Release artifacts are reproducible, signed, documented, reviewed, and supply-chain complete.
    ReleaseArtifacts = 24,
    /// No quarantine, ignored failure, blocker, undocumented unsafe, or placeholder remains.
    NoReleaseDebt = 25,
}

impl AcceptanceCriterion {
    /// Returns the stable one-based architecture criterion identifier.
    #[must_use]
    pub const fn stable_id(self) -> u8 { self as u8 }
}

/// Canonical definition of one acceptance criterion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CriterionDefinition {
    /// Stable criterion identity.
    pub criterion: AcceptanceCriterion,
    /// Stable short title.
    pub title: &'static str,
    /// Normative criterion statement.
    pub statement: &'static str,
}

impl CriterionDefinition {
    /// Returns the stable criterion identity.
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion { self.criterion }

    /// Returns the stable short title.
    #[must_use]
    pub const fn title(&self) -> &'static str { self.title }

    /// Returns the normative criterion statement.
    #[must_use]
    pub const fn statement(&self) -> &'static str { self.statement }
}

/// Canonical closed catalog, in stable criterion-ID order.
pub const PRODUCTION_CRITERIA: [CriterionDefinition; 25] = [
    CriterionDefinition { criterion: AcceptanceCriterion::CleanTierOneSuite, title: "clean-tier-one-suite", statement: "A clean checkout passes formatting, strict Clippy, unit, integration, documentation, compatibility, property, concurrency, Miri-eligible, fuzz-smoke, security, and end-to-end suites on every tier-one platform." },
    CriterionDefinition { criterion: AcceptanceCriterion::VerifiedWorkspaceBuild, title: "verified-workspace-build", statement: "The clean locked workspace passes Verus verification and the verified release build without an unapproved trusted construct." },
    CriterionDefinition { criterion: AcceptanceCriterion::ProofObligationInventory, title: "proof-obligation-inventory", statement: "Every deterministic decision function is verified or has a narrowly approved exclusion with compensating evidence." },
    CriterionDefinition { criterion: AcceptanceCriterion::TrustedConstructAudit, title: "trusted-construct-audit", statement: "Machine checks reject assume, admit, axiom, external-body, and equivalent constructs outside the allowlist, and every allowlisted entry has threat analysis and refinement evidence." },
    CriterionDefinition { criterion: AcceptanceCriterion::PrivilegedConstruction, title: "privileged-construction", statement: "Model, tool, and ordinary Rust callers cannot construct privileged tokens, accepted states, closed findings, current evidence, or promoted harness states without verified transitions." },
    CriterionDefinition { criterion: AcceptanceCriterion::IllegalLifecycleEdges, title: "illegal-lifecycle-edges", statement: "Every illegal lifecycle edge is attempted and consistently rejected by Verus, property, and protocol-conformance evidence." },
    CriterionDefinition { criterion: AcceptanceCriterion::CrashRecovery, title: "crash-recovery", statement: "Power-loss and crash injection at every journal, blob, snapshot, lease, patch, gate, and promotion commit boundary recovers without divergence or false success." },
    CriterionDefinition { criterion: AcceptanceCriterion::DeterministicReplay, title: "deterministic-replay", statement: "Replay from an empty projection database reproduces authoritative state and every acceptance decision byte-for-byte for the compatibility corpus." },
    CriterionDefinition { criterion: AcceptanceCriterion::MaliciousRepository, title: "malicious-repository", statement: "The malicious-repository suite covers traversal, races, repository tricks, aliases, device paths, injection, poisoned instructions, oversized output, terminal escapes, and secret exfiltration." },
    CriterionDefinition { criterion: AcceptanceCriterion::NativeSandboxSecurity, title: "native-sandbox-security", statement: "Linux, macOS, and Windows sandboxes pass the common capability suite and independent escape-focused security review." },
    CriterionDefinition { criterion: AcceptanceCriterion::RoleIsolation, title: "role-isolation", statement: "Read-only actors cannot mutate and writable actors cannot approve or waive their own results." },
    CriterionDefinition { criterion: AcceptanceCriterion::EvidenceInvalidation, title: "evidence-invalidation", statement: "Any candidate mutation invalidates prior gate and review evidence, and stale evidence cannot accept another revision." },
    CriterionDefinition { criterion: AcceptanceCriterion::ExhaustionFailsClosed, title: "exhaustion-fails-closed", statement: "Budget, retry, and timeout exhaustion terminates without success and retains a complete evidence bundle." },
    CriterionDefinition { criterion: AcceptanceCriterion::DaemonLifecycleRecovery, title: "daemon-lifecycle-recovery", statement: "A daemon restart in every active phase resumes, reconciles, or explicitly fails owned tasks without orphaned authoritative work." },
    CriterionDefinition { criterion: AcceptanceCriterion::ProviderContracts, title: "provider-contracts", statement: "Provider tests cover interruption, duplicates, reordering, retry-after, malformed structured output, partial tool calls, cancellation, and idempotent retry." },
    CriterionDefinition { criterion: AcceptanceCriterion::MigrationAndExport, title: "migration-and-export", statement: "Every historical schema fixture migrates forward, corrupt or divergent journals fail, and a portable evidence bundle can be exported." },
    CriterionDefinition { criterion: AcceptanceCriterion::EvolutionIsolation, title: "evolution-isolation", statement: "Evolution red-team tests prevent sealed-answer access, evaluator mutation, profile or policy drift, and self-promotion." },
    CriterionDefinition { criterion: AcceptanceCriterion::PromotionAndRollback, title: "promotion-and-rollback", statement: "Promotion requires all immutable statistical, correctness, safety, resource, and authority gates, and rollback atomically preserves both histories." },
    CriterionDefinition { criterion: AcceptanceCriterion::ObservabilityAndRedaction, title: "observability-and-redaction", statement: "Observability cites source IDs, distinguishes infrastructure from task failure, and redacts seeded secrets from default logs and exports." },
    CriterionDefinition { criterion: AcceptanceCriterion::LoadAndSoak, title: "load-and-soak", statement: "Load and soak tests meet the documented concurrency, latency, streaming, memory, cancellation, and recovery objectives." },
    CriterionDefinition { criterion: AcceptanceCriterion::PublicSurfaceDocumentation, title: "public-surface-documentation", statement: "Every public command and protocol method has references, examples, stable errors, and end-to-end tests." },
    CriterionDefinition { criterion: AcceptanceCriterion::ArchitectureIntegrity, title: "architecture-integrity", statement: "Architecture checks reject dependency cycles, forbidden upward dependencies, god roots, unowned generation, and implementation-crate API leakage." },
    CriterionDefinition { criterion: AcceptanceCriterion::RepresentativeCampaign, title: "representative-campaign", statement: "The final independent writer/reviewer/fixer campaign completes Rust, TypeScript, Python, Java, and mixed tasks reproducibly without manual repair." },
    CriterionDefinition { criterion: AcceptanceCriterion::ReleaseArtifacts, title: "release-artifacts", statement: "Artifacts are reproducible, signed, accompanied by SBOM, provenance, notices, migration and recovery documentation, and completed security review." },
    CriterionDefinition { criterion: AcceptanceCriterion::NoReleaseDebt, title: "no-release-debt", statement: "No quarantined test, ignored failure, unresolved release blocker, undocumented unsafe block, or placeholder production implementation remains." },
];

/// Required evidence artifact in canonical stable-ID order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum EvidenceRequirement {
    /// Complete Gate A result for the exact release commit.
    GateA = 1,
    /// Complete ordinary quality matrix on every tier-one platform.
    FoundationQualityMatrix = 2,
    /// Complete workspace Verus verification.
    FoundationVerusVerify = 3,
    /// Complete verified release build.
    FoundationVerusBuild = 4,
    /// Proof-obligation inventory.
    ProofInventory = 5,
    /// Trusted-construct and allowlist audit.
    TrustBoundaryAudit = 6,
    /// Ordinary-wrapper and privileged-construction conformance.
    PrivilegedConstructionConformance = 7,
    /// Illegal lifecycle-edge matrix.
    IllegalLifecycleEdgeMatrix = 8,
    /// Commit-boundary power-loss and crash campaign.
    CrashInjectionCampaign = 9,
    /// Empty-projection deterministic replay corpus.
    DeterministicReplayCorpus = 10,
    /// Malicious-repository security suite.
    MaliciousRepositorySuite = 11,
    /// Linux native sandbox qualification.
    LinuxNativeQualification = 12,
    /// macOS native sandbox qualification.
    MacOsNativeQualification = 13,
    /// Windows native sandbox qualification.
    WindowsNativeQualification = 14,
    /// Independent native sandbox escape review.
    SandboxEscapeReview = 15,
    /// Writer/reviewer/fixer isolation matrix.
    RoleIsolationMatrix = 16,
    /// Candidate-mutation evidence-invalidation matrix.
    EvidenceInvalidationMatrix = 17,
    /// Budget/retry/timeout fail-closed matrix.
    ExhaustionFailClosedMatrix = 18,
    /// Active-phase daemon restart campaign.
    DaemonRecoveryCampaign = 19,
    /// Provider interruption and malformed-stream contracts.
    ProviderContractMatrix = 20,
    /// Historical schema migration corpus.
    MigrationCorpus = 21,
    /// Portable evidence export round-trip.
    EvidenceExport = 22,
    /// Sealed-evaluator evolution red-team campaign.
    EvolutionRedTeam = 23,
    /// Immutable multi-objective promotion gate results.
    PromotionGateMatrix = 24,
    /// Atomic rollback and history-preservation evidence.
    AtomicRollback = 25,
    /// Source-cited failure-classified observability evidence.
    ObservabilityCitations = 26,
    /// Seeded-secret redaction evidence.
    SecretRedaction = 27,
    /// Representative load SLO report.
    LoadSlo = 28,
    /// Required eight-hour soak report.
    EightHourSoak = 29,
    /// Complete public reference documentation inventory.
    PublicReferenceDocumentation = 30,
    /// Public command and protocol end-to-end matrix.
    CommandProtocolEndToEnd = 31,
    /// Dependency, ownership, root-size, generation, and API architecture audit.
    ArchitectureAudit = 32,
    /// Independent multi-language representative campaign.
    RepresentativeCampaign = 33,
    /// Reproducible artifact comparison.
    ReproducibleArtifacts = 34,
    /// Artifact signature inventory and verification.
    ArtifactSignatures = 35,
    /// Software bill of materials.
    Sbom = 36,
    /// Release provenance attestation.
    Provenance = 37,
    /// Complete license notices.
    LicenseNotices = 38,
    /// Migration and recovery documentation.
    MigrationRecoveryDocumentation = 39,
    /// Completed independent security review.
    CompletedSecurityReview = 40,
    /// Quarantined and ignored-test audit.
    TestQuarantineAudit = 41,
    /// Release-blocking finding audit.
    ReleaseFindingAudit = 42,
    /// Unsafe-code documentation inventory.
    UnsafeInventory = 43,
    /// Production placeholder scan.
    PlaceholderAudit = 44,
}

impl EvidenceRequirement {
    /// Returns the stable one-based evidence-requirement identifier.
    #[must_use]
    pub const fn stable_id(self) -> u8 { self as u8 }

    /// Returns the acceptance criterion supported by this evidence.
    #[must_use]
    pub const fn criterion(self) -> AcceptanceCriterion {
        match self {
            Self::GateA | Self::FoundationQualityMatrix => AcceptanceCriterion::CleanTierOneSuite,
            Self::FoundationVerusVerify | Self::FoundationVerusBuild => AcceptanceCriterion::VerifiedWorkspaceBuild,
            Self::ProofInventory => AcceptanceCriterion::ProofObligationInventory,
            Self::TrustBoundaryAudit => AcceptanceCriterion::TrustedConstructAudit,
            Self::PrivilegedConstructionConformance => AcceptanceCriterion::PrivilegedConstruction,
            Self::IllegalLifecycleEdgeMatrix => AcceptanceCriterion::IllegalLifecycleEdges,
            Self::CrashInjectionCampaign => AcceptanceCriterion::CrashRecovery,
            Self::DeterministicReplayCorpus => AcceptanceCriterion::DeterministicReplay,
            Self::MaliciousRepositorySuite => AcceptanceCriterion::MaliciousRepository,
            Self::LinuxNativeQualification | Self::MacOsNativeQualification | Self::WindowsNativeQualification | Self::SandboxEscapeReview => AcceptanceCriterion::NativeSandboxSecurity,
            Self::RoleIsolationMatrix => AcceptanceCriterion::RoleIsolation,
            Self::EvidenceInvalidationMatrix => AcceptanceCriterion::EvidenceInvalidation,
            Self::ExhaustionFailClosedMatrix => AcceptanceCriterion::ExhaustionFailsClosed,
            Self::DaemonRecoveryCampaign => AcceptanceCriterion::DaemonLifecycleRecovery,
            Self::ProviderContractMatrix => AcceptanceCriterion::ProviderContracts,
            Self::MigrationCorpus | Self::EvidenceExport => AcceptanceCriterion::MigrationAndExport,
            Self::EvolutionRedTeam => AcceptanceCriterion::EvolutionIsolation,
            Self::PromotionGateMatrix | Self::AtomicRollback => AcceptanceCriterion::PromotionAndRollback,
            Self::ObservabilityCitations | Self::SecretRedaction => AcceptanceCriterion::ObservabilityAndRedaction,
            Self::LoadSlo | Self::EightHourSoak => AcceptanceCriterion::LoadAndSoak,
            Self::PublicReferenceDocumentation | Self::CommandProtocolEndToEnd => AcceptanceCriterion::PublicSurfaceDocumentation,
            Self::ArchitectureAudit => AcceptanceCriterion::ArchitectureIntegrity,
            Self::RepresentativeCampaign => AcceptanceCriterion::RepresentativeCampaign,
            Self::ReproducibleArtifacts | Self::ArtifactSignatures | Self::Sbom | Self::Provenance | Self::LicenseNotices | Self::MigrationRecoveryDocumentation | Self::CompletedSecurityReview => AcceptanceCriterion::ReleaseArtifacts,
            Self::TestQuarantineAudit | Self::ReleaseFindingAudit | Self::UnsafeInventory | Self::PlaceholderAudit => AcceptanceCriterion::NoReleaseDebt,
        }
    }

    /// Returns the only source class allowed to satisfy this requirement.
    #[must_use]
    pub const fn source_kind(self) -> (source_kind: EvidenceSourceKind)
        ensures source_kind == self.spec_source_kind()
    {
        match self {
            Self::GateA => EvidenceSourceKind::GateA,
            Self::FoundationQualityMatrix | Self::FoundationVerusVerify | Self::FoundationVerusBuild | Self::ProofInventory | Self::TrustBoundaryAudit | Self::PrivilegedConstructionConformance | Self::IllegalLifecycleEdgeMatrix | Self::ArchitectureAudit | Self::UnsafeInventory | Self::PlaceholderAudit => EvidenceSourceKind::Foundation,
            Self::LinuxNativeQualification | Self::MacOsNativeQualification | Self::WindowsNativeQualification => EvidenceSourceKind::NativeRunner,
            Self::SandboxEscapeReview | Self::MaliciousRepositorySuite | Self::CompletedSecurityReview => EvidenceSourceKind::Security,
            Self::CrashInjectionCampaign | Self::ExhaustionFailClosedMatrix | Self::DaemonRecoveryCampaign => EvidenceSourceKind::Resilience,
            Self::DeterministicReplayCorpus | Self::EvidenceInvalidationMatrix | Self::ProviderContractMatrix | Self::CommandProtocolEndToEnd => EvidenceSourceKind::Conformance,
            Self::RoleIsolationMatrix | Self::ReleaseFindingAudit | Self::TestQuarantineAudit => EvidenceSourceKind::Review,
            Self::MigrationCorpus | Self::EvidenceExport | Self::MigrationRecoveryDocumentation => EvidenceSourceKind::Migration,
            Self::EvolutionRedTeam | Self::PromotionGateMatrix | Self::AtomicRollback => EvidenceSourceKind::Evolution,
            Self::ObservabilityCitations | Self::SecretRedaction => EvidenceSourceKind::Observability,
            Self::LoadSlo => EvidenceSourceKind::Performance,
            Self::EightHourSoak => EvidenceSourceKind::Soak,
            Self::PublicReferenceDocumentation => EvidenceSourceKind::Documentation,
            Self::RepresentativeCampaign => EvidenceSourceKind::RepresentativeCampaign,
            Self::ReproducibleArtifacts => EvidenceSourceKind::Reproducibility,
            Self::ArtifactSignatures => EvidenceSourceKind::Signature,
            Self::Sbom => EvidenceSourceKind::Sbom,
            Self::Provenance => EvidenceSourceKind::Provenance,
            Self::LicenseNotices => EvidenceSourceKind::License,
        }
    }

    /// Specification view of the required source class.
    pub open spec fn spec_source_kind(self) -> EvidenceSourceKind {
        match self {
            Self::GateA => EvidenceSourceKind::GateA,
            Self::FoundationQualityMatrix | Self::FoundationVerusVerify | Self::FoundationVerusBuild | Self::ProofInventory | Self::TrustBoundaryAudit | Self::PrivilegedConstructionConformance | Self::IllegalLifecycleEdgeMatrix | Self::ArchitectureAudit | Self::UnsafeInventory | Self::PlaceholderAudit => EvidenceSourceKind::Foundation,
            Self::LinuxNativeQualification | Self::MacOsNativeQualification | Self::WindowsNativeQualification => EvidenceSourceKind::NativeRunner,
            Self::SandboxEscapeReview | Self::MaliciousRepositorySuite | Self::CompletedSecurityReview => EvidenceSourceKind::Security,
            Self::CrashInjectionCampaign | Self::ExhaustionFailClosedMatrix | Self::DaemonRecoveryCampaign => EvidenceSourceKind::Resilience,
            Self::DeterministicReplayCorpus | Self::EvidenceInvalidationMatrix | Self::ProviderContractMatrix | Self::CommandProtocolEndToEnd => EvidenceSourceKind::Conformance,
            Self::RoleIsolationMatrix | Self::ReleaseFindingAudit | Self::TestQuarantineAudit => EvidenceSourceKind::Review,
            Self::MigrationCorpus | Self::EvidenceExport | Self::MigrationRecoveryDocumentation => EvidenceSourceKind::Migration,
            Self::EvolutionRedTeam | Self::PromotionGateMatrix | Self::AtomicRollback => EvidenceSourceKind::Evolution,
            Self::ObservabilityCitations | Self::SecretRedaction => EvidenceSourceKind::Observability,
            Self::LoadSlo => EvidenceSourceKind::Performance,
            Self::EightHourSoak => EvidenceSourceKind::Soak,
            Self::PublicReferenceDocumentation => EvidenceSourceKind::Documentation,
            Self::RepresentativeCampaign => EvidenceSourceKind::RepresentativeCampaign,
            Self::ReproducibleArtifacts => EvidenceSourceKind::Reproducibility,
            Self::ArtifactSignatures => EvidenceSourceKind::Signature,
            Self::Sbom => EvidenceSourceKind::Sbom,
            Self::Provenance => EvidenceSourceKind::Provenance,
            Self::LicenseNotices => EvidenceSourceKind::License,
        }
    }
}

/// Authenticated origin class of an evidence observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceSourceKind {
    /// Repository Gate A workflow.
    GateA,
    /// Pinned foundation toolchain and repository checks.
    Foundation,
    /// Native tier-one host runner.
    NativeRunner,
    /// Security qualification or independent security reviewer.
    Security,
    /// H1 resilience runner.
    Resilience,
    /// Runtime-neutral conformance harness.
    Conformance,
    /// Independent review system.
    Review,
    /// Migration and portable-evidence tooling.
    Migration,
    /// Evolution qualification runner.
    Evolution,
    /// Telemetry and trace qualification.
    Observability,
    /// H3 load runner.
    Performance,
    /// Dedicated H3 soak runner.
    Soak,
    /// Documentation inventory.
    Documentation,
    /// Final representative campaign runner.
    RepresentativeCampaign,
    /// Reproducible-build comparison.
    Reproducibility,
    /// Artifact signing service or offline signer.
    Signature,
    /// SBOM generator and reviewer.
    Sbom,
    /// Provenance attestation producer.
    Provenance,
    /// License policy and notice inventory.
    License,
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evidence evaluation compares closed requirement identities across modules"
)]
pub(crate) const fn requirements_equal(
    left: EvidenceRequirement,
    right: EvidenceRequirement,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (EvidenceRequirement::GateA, EvidenceRequirement::GateA)
        | (EvidenceRequirement::FoundationQualityMatrix, EvidenceRequirement::FoundationQualityMatrix)
        | (EvidenceRequirement::FoundationVerusVerify, EvidenceRequirement::FoundationVerusVerify)
        | (EvidenceRequirement::FoundationVerusBuild, EvidenceRequirement::FoundationVerusBuild)
        | (EvidenceRequirement::ProofInventory, EvidenceRequirement::ProofInventory)
        | (EvidenceRequirement::TrustBoundaryAudit, EvidenceRequirement::TrustBoundaryAudit)
        | (EvidenceRequirement::PrivilegedConstructionConformance, EvidenceRequirement::PrivilegedConstructionConformance)
        | (EvidenceRequirement::IllegalLifecycleEdgeMatrix, EvidenceRequirement::IllegalLifecycleEdgeMatrix)
        | (EvidenceRequirement::CrashInjectionCampaign, EvidenceRequirement::CrashInjectionCampaign)
        | (EvidenceRequirement::DeterministicReplayCorpus, EvidenceRequirement::DeterministicReplayCorpus)
        | (EvidenceRequirement::MaliciousRepositorySuite, EvidenceRequirement::MaliciousRepositorySuite)
        | (EvidenceRequirement::LinuxNativeQualification, EvidenceRequirement::LinuxNativeQualification)
        | (EvidenceRequirement::MacOsNativeQualification, EvidenceRequirement::MacOsNativeQualification)
        | (EvidenceRequirement::WindowsNativeQualification, EvidenceRequirement::WindowsNativeQualification)
        | (EvidenceRequirement::SandboxEscapeReview, EvidenceRequirement::SandboxEscapeReview)
        | (EvidenceRequirement::RoleIsolationMatrix, EvidenceRequirement::RoleIsolationMatrix)
        | (EvidenceRequirement::EvidenceInvalidationMatrix, EvidenceRequirement::EvidenceInvalidationMatrix)
        | (EvidenceRequirement::ExhaustionFailClosedMatrix, EvidenceRequirement::ExhaustionFailClosedMatrix)
        | (EvidenceRequirement::DaemonRecoveryCampaign, EvidenceRequirement::DaemonRecoveryCampaign)
        | (EvidenceRequirement::ProviderContractMatrix, EvidenceRequirement::ProviderContractMatrix)
        | (EvidenceRequirement::MigrationCorpus, EvidenceRequirement::MigrationCorpus)
        | (EvidenceRequirement::EvidenceExport, EvidenceRequirement::EvidenceExport)
        | (EvidenceRequirement::EvolutionRedTeam, EvidenceRequirement::EvolutionRedTeam)
        | (EvidenceRequirement::PromotionGateMatrix, EvidenceRequirement::PromotionGateMatrix)
        | (EvidenceRequirement::AtomicRollback, EvidenceRequirement::AtomicRollback)
        | (EvidenceRequirement::ObservabilityCitations, EvidenceRequirement::ObservabilityCitations)
        | (EvidenceRequirement::SecretRedaction, EvidenceRequirement::SecretRedaction)
        | (EvidenceRequirement::LoadSlo, EvidenceRequirement::LoadSlo)
        | (EvidenceRequirement::EightHourSoak, EvidenceRequirement::EightHourSoak)
        | (EvidenceRequirement::PublicReferenceDocumentation, EvidenceRequirement::PublicReferenceDocumentation)
        | (EvidenceRequirement::CommandProtocolEndToEnd, EvidenceRequirement::CommandProtocolEndToEnd)
        | (EvidenceRequirement::ArchitectureAudit, EvidenceRequirement::ArchitectureAudit)
        | (EvidenceRequirement::RepresentativeCampaign, EvidenceRequirement::RepresentativeCampaign)
        | (EvidenceRequirement::ReproducibleArtifacts, EvidenceRequirement::ReproducibleArtifacts)
        | (EvidenceRequirement::ArtifactSignatures, EvidenceRequirement::ArtifactSignatures)
        | (EvidenceRequirement::Sbom, EvidenceRequirement::Sbom)
        | (EvidenceRequirement::Provenance, EvidenceRequirement::Provenance)
        | (EvidenceRequirement::LicenseNotices, EvidenceRequirement::LicenseNotices)
        | (EvidenceRequirement::MigrationRecoveryDocumentation, EvidenceRequirement::MigrationRecoveryDocumentation)
        | (EvidenceRequirement::CompletedSecurityReview, EvidenceRequirement::CompletedSecurityReview)
        | (EvidenceRequirement::TestQuarantineAudit, EvidenceRequirement::TestQuarantineAudit)
        | (EvidenceRequirement::ReleaseFindingAudit, EvidenceRequirement::ReleaseFindingAudit)
        | (EvidenceRequirement::UnsafeInventory, EvidenceRequirement::UnsafeInventory)
        | (EvidenceRequirement::PlaceholderAudit, EvidenceRequirement::PlaceholderAudit))
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "verified evidence evaluation compares closed source identities across modules"
)]
pub(crate) const fn source_kinds_equal(
    left: EvidenceSourceKind,
    right: EvidenceSourceKind,
) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (EvidenceSourceKind::GateA, EvidenceSourceKind::GateA)
        | (EvidenceSourceKind::Foundation, EvidenceSourceKind::Foundation)
        | (EvidenceSourceKind::NativeRunner, EvidenceSourceKind::NativeRunner)
        | (EvidenceSourceKind::Security, EvidenceSourceKind::Security)
        | (EvidenceSourceKind::Resilience, EvidenceSourceKind::Resilience)
        | (EvidenceSourceKind::Conformance, EvidenceSourceKind::Conformance)
        | (EvidenceSourceKind::Review, EvidenceSourceKind::Review)
        | (EvidenceSourceKind::Migration, EvidenceSourceKind::Migration)
        | (EvidenceSourceKind::Evolution, EvidenceSourceKind::Evolution)
        | (EvidenceSourceKind::Observability, EvidenceSourceKind::Observability)
        | (EvidenceSourceKind::Performance, EvidenceSourceKind::Performance)
        | (EvidenceSourceKind::Soak, EvidenceSourceKind::Soak)
        | (EvidenceSourceKind::Documentation, EvidenceSourceKind::Documentation)
        | (EvidenceSourceKind::RepresentativeCampaign, EvidenceSourceKind::RepresentativeCampaign)
        | (EvidenceSourceKind::Reproducibility, EvidenceSourceKind::Reproducibility)
        | (EvidenceSourceKind::Signature, EvidenceSourceKind::Signature)
        | (EvidenceSourceKind::Sbom, EvidenceSourceKind::Sbom)
        | (EvidenceSourceKind::Provenance, EvidenceSourceKind::Provenance)
        | (EvidenceSourceKind::License, EvidenceSourceKind::License))
}

/// Canonical evidence requirements, in stable requirement-ID order.
pub const REQUIRED_EVIDENCE: [EvidenceRequirement; 44] = [
    EvidenceRequirement::GateA,
    EvidenceRequirement::FoundationQualityMatrix,
    EvidenceRequirement::FoundationVerusVerify,
    EvidenceRequirement::FoundationVerusBuild,
    EvidenceRequirement::ProofInventory,
    EvidenceRequirement::TrustBoundaryAudit,
    EvidenceRequirement::PrivilegedConstructionConformance,
    EvidenceRequirement::IllegalLifecycleEdgeMatrix,
    EvidenceRequirement::CrashInjectionCampaign,
    EvidenceRequirement::DeterministicReplayCorpus,
    EvidenceRequirement::MaliciousRepositorySuite,
    EvidenceRequirement::LinuxNativeQualification,
    EvidenceRequirement::MacOsNativeQualification,
    EvidenceRequirement::WindowsNativeQualification,
    EvidenceRequirement::SandboxEscapeReview,
    EvidenceRequirement::RoleIsolationMatrix,
    EvidenceRequirement::EvidenceInvalidationMatrix,
    EvidenceRequirement::ExhaustionFailClosedMatrix,
    EvidenceRequirement::DaemonRecoveryCampaign,
    EvidenceRequirement::ProviderContractMatrix,
    EvidenceRequirement::MigrationCorpus,
    EvidenceRequirement::EvidenceExport,
    EvidenceRequirement::EvolutionRedTeam,
    EvidenceRequirement::PromotionGateMatrix,
    EvidenceRequirement::AtomicRollback,
    EvidenceRequirement::ObservabilityCitations,
    EvidenceRequirement::SecretRedaction,
    EvidenceRequirement::LoadSlo,
    EvidenceRequirement::EightHourSoak,
    EvidenceRequirement::PublicReferenceDocumentation,
    EvidenceRequirement::CommandProtocolEndToEnd,
    EvidenceRequirement::ArchitectureAudit,
    EvidenceRequirement::RepresentativeCampaign,
    EvidenceRequirement::ReproducibleArtifacts,
    EvidenceRequirement::ArtifactSignatures,
    EvidenceRequirement::Sbom,
    EvidenceRequirement::Provenance,
    EvidenceRequirement::LicenseNotices,
    EvidenceRequirement::MigrationRecoveryDocumentation,
    EvidenceRequirement::CompletedSecurityReview,
    EvidenceRequirement::TestQuarantineAudit,
    EvidenceRequirement::ReleaseFindingAudit,
    EvidenceRequirement::UnsafeInventory,
    EvidenceRequirement::PlaceholderAudit,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_catalog_ids_are_contiguous() {
        let mut index = 0;
        while index < PRODUCTION_CRITERIA.len() {
            assert_eq!(usize::from(PRODUCTION_CRITERIA[index].criterion().stable_id()), index + 1);
            index += 1;
        }
        let mut requirement_index = 0;
        while requirement_index < REQUIRED_EVIDENCE.len() {
            assert_eq!(
                usize::from(REQUIRED_EVIDENCE[requirement_index].stable_id()),
                requirement_index + 1,
            );
            requirement_index += 1;
        }
    }

    #[test]
    fn criterion_twenty_four_requires_every_release_artifact_class() {
        let mut observed = 0;
        for requirement in REQUIRED_EVIDENCE {
            if requirement.criterion() == AcceptanceCriterion::ReleaseArtifacts {
                observed += 1;
            }
        }
        assert_eq!(observed, 7);
    }
}

} // verus!
