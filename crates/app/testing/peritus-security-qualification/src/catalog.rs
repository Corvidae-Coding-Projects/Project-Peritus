//! Closed H0 probe catalog with literal architecture traceability.

mod data;

use peritus_security_policy::{AcceptanceCriterion, SecurityRequirement};

/// Number of probes in the immutable H0 production campaign.
pub const H0_PRODUCTION_PROBE_COUNT: usize = 42;

/// Native target on which a probe must execute.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProbeTarget {
    /// Any tier-one native host; the case does not assert a specific OS backend.
    TierOneHost,
    /// Native Linux sandbox host.
    Linux,
    /// Native macOS sandbox host.
    Macos,
    /// Native Windows sandbox host.
    Windows,
}

impl ProbeTarget {
    /// Returns the stable evidence code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TierOneHost => "tier-one-host",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Windows => "windows",
        }
    }
}

/// Stable identity for one realistic H0 security probe.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProbeId {
    /// Reject repository-relative and absolute traversal.
    RepositoryTraversal,
    /// Reject symlink replacement and target races.
    SymlinkRace,
    /// Contain malicious submodule paths and metadata.
    SubmoduleEscape,
    /// Contain linked-worktree and external gitdir indirection.
    WorktreeEscape,
    /// Reject case-folded aliases of protected paths.
    CaseAliasCollision,
    /// Reject Unix and Windows device namespace paths.
    DevicePath,
    /// Keep repository text out of shell authority.
    ShellInjection,
    /// Preserve provenance under poisoned repository instructions.
    PoisonedRepositoryInstructions,
    /// Bound hostile process and tool output.
    OversizedOutput,
    /// Sanitize terminal control and hyperlink sequences.
    TerminalEscape,
    /// Deny seeded-secret exfiltration routes.
    SecretExfiltration,
    /// Exercise the native Linux sandbox capability contract.
    LinuxSandboxCapabilities,
    /// Exercise the native macOS sandbox capability contract.
    MacosSandboxCapabilities,
    /// Exercise the native Windows sandbox capability contract.
    WindowsSandboxCapabilities,
    /// Exercise reviewed native sandbox escape attempts.
    SandboxEscape,
    /// Prove native default-deny network behavior.
    NetworkDefaultDeny,
    /// Enforce digest-bound plugin capabilities.
    PluginCapabilityScope,
    /// Enforce current actor-bound MCP capabilities.
    McpCapabilityScope,
    /// Prove reviewer mutation denial.
    ReviewerReadOnly,
    /// Prove a fixer cannot approve its own remediation.
    FixerCannotApprove,
    /// Prove a writer cannot waive findings against its work.
    WriterCannotWaive,
    /// Invalidate all prior evidence after candidate mutation.
    CandidateMutationInvalidation,
    /// Deny evolution access to sealed answers.
    SealedAnswerDenial,
    /// Deny evolution writes to evaluators and policy.
    EvaluatorMutationDenial,
    /// Deny unauthorized model and resource profile mutation.
    ProfileMutationDenial,
    /// Deny candidate self-promotion.
    SelfPromotionDenial,
    /// Isolate candidate, baseline, evaluator, and external provenance.
    EvolutionCampaignIsolation,
    /// Bind promotion gates to immutable candidate and baseline revisions.
    PromotionGateBinding,
    /// Preserve both histories across atomic rollback.
    AtomicRollbackHistory,
    /// Require source event and artifact citations.
    EvidenceCitation,
    /// Separate infrastructure failure from task failure.
    InfrastructureFailureTaxonomy,
    /// Redact seeded secrets from default evidence surfaces.
    SecretRedaction,
    /// Rebuild dependencies from the locked auditable graph.
    DependencyReproducibility,
    /// Verify release signatures, SBOM, provenance, and licenses.
    ReleaseSignatureSbom,
    /// Verify migration and recovery documentation packaging.
    MigrationRecoveryDocumentation,
    /// Reconcile all unsafe code and safety evidence.
    UnsafeInventory,
    /// Reconcile trusted code, tools, and exclusions.
    TcbInventory,
    /// Reject quarantined failures and production placeholders.
    NoQuarantinedOrPlaceholderProduction,
    /// Reconcile findings through remediation and independent retest.
    FindingLifecycle,
    /// Bound cancellation and remove the complete owned subject.
    CancellationAndTreeCleanup,
    /// Reconcile current threats, assets, boundaries, and owners.
    ThreatInventory,
    /// Reconcile requirement, control, probe, and evidence mappings.
    ControlInventory,
}

impl ProbeId {
    /// Returns the stable lowercase dotted probe identity.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryTraversal => "h0.repository.traversal",
            Self::SymlinkRace => "h0.repository.symlink-race",
            Self::SubmoduleEscape => "h0.repository.submodule-escape",
            Self::WorktreeEscape => "h0.repository.worktree-escape",
            Self::CaseAliasCollision => "h0.repository.case-alias",
            Self::DevicePath => "h0.repository.device-path",
            Self::ShellInjection => "h0.repository.shell-injection",
            Self::PoisonedRepositoryInstructions => "h0.repository.poisoned-instructions",
            Self::OversizedOutput => "h0.repository.oversized-output",
            Self::TerminalEscape => "h0.repository.terminal-escape",
            Self::SecretExfiltration => "h0.repository.secret-exfiltration",
            Self::LinuxSandboxCapabilities => "h0.sandbox.linux-capabilities",
            Self::MacosSandboxCapabilities => "h0.sandbox.macos-capabilities",
            Self::WindowsSandboxCapabilities => "h0.sandbox.windows-capabilities",
            Self::SandboxEscape => "h0.sandbox.escape-review",
            Self::NetworkDefaultDeny => "h0.network.default-deny",
            Self::PluginCapabilityScope => "h0.plugin.capability-scope",
            Self::McpCapabilityScope => "h0.mcp.capability-scope",
            Self::ReviewerReadOnly => "h0.role.reviewer-read-only",
            Self::FixerCannotApprove => "h0.role.fixer-cannot-approve",
            Self::WriterCannotWaive => "h0.role.writer-cannot-waive",
            Self::CandidateMutationInvalidation => "h0.freshness.candidate-mutation",
            Self::SealedAnswerDenial => "h0.evolution.sealed-answer-denial",
            Self::EvaluatorMutationDenial => "h0.evolution.evaluator-mutation-denial",
            Self::ProfileMutationDenial => "h0.evolution.profile-mutation-denial",
            Self::SelfPromotionDenial => "h0.evolution.self-promotion-denial",
            Self::EvolutionCampaignIsolation => "h0.evolution.campaign-isolation",
            Self::PromotionGateBinding => "h0.promotion.gate-binding",
            Self::AtomicRollbackHistory => "h0.promotion.atomic-rollback-history",
            Self::EvidenceCitation => "h0.observability.evidence-citation",
            Self::InfrastructureFailureTaxonomy => "h0.observability.failure-taxonomy",
            Self::SecretRedaction => "h0.observability.secret-redaction",
            Self::DependencyReproducibility => "h0.supply-chain.dependency-reproducibility",
            Self::ReleaseSignatureSbom => "h0.supply-chain.signature-sbom",
            Self::MigrationRecoveryDocumentation => "h0.supply-chain.migration-recovery-docs",
            Self::UnsafeInventory => "h0.inventory.unsafe",
            Self::TcbInventory => "h0.inventory.tcb",
            Self::NoQuarantinedOrPlaceholderProduction => "h0.release.no-quarantine-placeholder",
            Self::FindingLifecycle => "h0.review.finding-lifecycle",
            Self::CancellationAndTreeCleanup => "h0.resources.cancellation-cleanup",
            Self::ThreatInventory => "h0.inventory.threats",
            Self::ControlInventory => "h0.inventory.controls",
        }
    }
}

/// Immutable probe contract and traceability row.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProbeSpec {
    id: ProbeId,
    target: ProbeTarget,
    requirement: SecurityRequirement,
    criterion: AcceptanceCriterion,
    description: &'static str,
}

impl ProbeSpec {
    /// Returns the stable probe identity.
    #[must_use]
    pub const fn id(self) -> ProbeId {
        self.id
    }
    /// Returns the required native target.
    #[must_use]
    pub const fn target(self) -> ProbeTarget {
        self.target
    }
    /// Returns the literal R-SEC obligation covered by the probe.
    #[must_use]
    pub const fn requirement(self) -> SecurityRequirement {
        self.requirement
    }
    /// Returns the literal numbered acceptance criterion covered by the probe.
    #[must_use]
    pub const fn criterion(self) -> AcceptanceCriterion {
        self.criterion
    }
    /// Returns the directly observable behavior required for pass.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }
    /// Reports whether passing evidence must name an observed native sandbox backend.
    #[must_use]
    pub const fn requires_native_sandbox(self) -> bool {
        matches!(
            self.id,
            ProbeId::LinuxSandboxCapabilities
                | ProbeId::MacosSandboxCapabilities
                | ProbeId::WindowsSandboxCapabilities
                | ProbeId::SandboxEscape
                | ProbeId::NetworkDefaultDeny
                | ProbeId::SecretExfiltration
        )
    }
    /// Returns the complete canonical H0 production catalog.
    #[must_use]
    pub const fn h0_production() -> &'static [Self; H0_PRODUCTION_PROBE_COUNT] {
        &data::PROBES
    }

    pub(super) const fn new(
        id: ProbeId,
        target: ProbeTarget,
        requirement: SecurityRequirement,
        criterion: AcceptanceCriterion,
        description: &'static str,
    ) -> Self {
        Self { id, target, requirement, criterion, description }
    }
}
