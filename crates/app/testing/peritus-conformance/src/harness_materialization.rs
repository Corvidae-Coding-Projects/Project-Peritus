//! Runtime-neutral E1 harness materialization conformance contract.

mod cases;

pub use cases::harness_suite;

/// One independently exercised E1 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessScenario {
    /// The committed manifest and recursive component inventory agree byte-exactly.
    ManifestInventory,
    /// Every closed component and protected controlled-asset class is represented.
    CompleteCatalog,
    /// Dependency, cycle, kind, version, digest, and feature constraints are enforced.
    GraphCompatibility,
    /// Declared and transitive authority remains within compiled ceilings.
    AuthorityConfinement,
    /// Successors cannot add, remove, or change protected controlled assets.
    ProtectedImmutability,
    /// Revisions are deterministic, content-addressed, and append-only.
    RevisionHistory,
    /// Forward materialization changes the exact owned target set through C1.
    ForwardMaterialization,
    /// Rollback accepts only an ancestor and creates a fresh workspace receipt.
    RollbackMaterialization,
    /// Every materialized byte is read from a verified finalized artifact.
    ArtifactIntegrity,
    /// All configured manifest, graph, history, state, and receipt bounds are enforced.
    BoundedState,
    /// Restart replay and exact command retry reproduce state without duplicate effects.
    Restart,
    /// Unknown, malformed, noncanonical, and trailing protocol bytes remain inert and rejected.
    MalformedProtocol,
    /// A panicking subject remains a typed failed conformance case.
    PanicContainment,
    /// Teardown failure stays visible and cannot manufacture a passing suite.
    TeardownIsolation,
}

/// Stable terminal observed from one E1 exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessTerminal {
    /// The requested registration or materialization completed exactly.
    Completed,
    /// Invalid input was rejected without a durable transition.
    Rejected,
    /// A typed execution failure was recorded.
    Failed,
    /// Ambiguous or conflicting durable state was quarantined.
    Quarantined,
}

/// Fixed realistic bounds supplied to one E1 case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HarnessConformanceFixture {
    scenario: HarnessScenario,
    maximum_components: u16,
    maximum_edges: u16,
    maximum_revisions: u16,
    maximum_receipts: u16,
}

impl HarnessConformanceFixture {
    pub(crate) const fn new(scenario: HarnessScenario) -> Self {
        Self {
            scenario,
            maximum_components: 32,
            maximum_edges: 128,
            maximum_revisions: 8,
            maximum_receipts: 8,
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> HarnessScenario {
        self.scenario
    }
    /// Returns the component ceiling.
    #[must_use]
    pub const fn maximum_components(self) -> u16 {
        self.maximum_components
    }
    /// Returns the dependency-edge ceiling.
    #[must_use]
    pub const fn maximum_edges(self) -> u16 {
        self.maximum_edges
    }
    /// Returns the retained revision ceiling.
    #[must_use]
    pub const fn maximum_revisions(self) -> u16 {
        self.maximum_revisions
    }
    /// Returns the retained receipt ceiling.
    #[must_use]
    pub const fn maximum_receipts(self) -> u16 {
        self.maximum_receipts
    }
}

/// Direct observations from one complete E1 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent harness, workspace, durability, and authority facts remain explicit"
)]
pub struct HarnessConformanceObservation {
    /// Terminal outcome.
    pub terminal: HarnessTerminal,
    /// Components retained by the checked graph.
    pub components: u16,
    /// Resolved dependency edges.
    pub edges: u16,
    /// Immutable revisions retained in history.
    pub revisions: u16,
    /// Settled materialization receipts retained by the projection.
    pub receipts: u16,
    /// C1 inventory exactly matched declarations and source bytes.
    pub manifest_inventory_exact: bool,
    /// The complete closed component/protection catalog was exercised.
    pub catalog_complete: bool,
    /// Invalid graph and compatibility forms were rejected.
    pub graph_rejections_exact: bool,
    /// Declared and dependency authority stayed within compiled ceilings.
    pub authority_confined: bool,
    /// Every protected-asset mutation attempt was rejected.
    pub protected_immutable: bool,
    /// Revision identity and append-only ancestry were deterministic.
    pub revision_history_exact: bool,
    /// C1 patch/candidate output matched the exact owned target inventory.
    pub workspace_materialization_exact: bool,
    /// Unrelated workspace paths were preserved.
    pub unrelated_paths_preserved: bool,
    /// Ancestor-only rollback produced a new exact receipt.
    pub rollback_exact: bool,
    /// Finalized artifact bytes, sizes, and digests were reverified.
    pub artifacts_verified: bool,
    /// All independent compiled and manifest bounds were enforced.
    pub bounds_enforced: bool,
    /// Genesis replay reproduced the complete live projection.
    pub replay_equivalent: bool,
    /// Exact retry produced no duplicate event, directive, patch, or receipt.
    pub idempotent_recovery: bool,
    /// Malformed protocol input remained inert and rejected.
    pub malformed_rejected: bool,
    /// Panic was contained as a case failure.
    pub panic_contained: bool,
    /// Teardown failure remained explicit and non-passing.
    pub teardown_explicit: bool,
    /// No registration, plan, receipt, rollback, or projection implied production promotion.
    pub no_implicit_promotion: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HarnessConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by an E1 production subject or development bridge.
pub trait HarnessConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &HarnessConformanceFixture,
    ) -> Result<HarnessConformanceObservation, HarnessConformanceError>;
}
