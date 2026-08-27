//! Closed fresh-subject H2 qualification scenario catalog.

/// Scenario family used for reporting and ownership.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScenarioCategory {
    /// Release archive, checksums, paths, and permissions.
    Package,
    /// Native supervisor and G0 transport lifecycle.
    Service,
    /// G1/G2 application behavior through the packaged G0 process.
    Application,
    /// C2 process ownership and C3 native sandbox behavior.
    Runtime,
    /// Upgrade, rollback, and uninstall preservation.
    Lifecycle,
}

/// Stable scenario identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScenarioId {
    /// Manifest and every artifact checksum match before installation.
    ArtifactIntegrity,
    /// Installed paths, types, ownership, and permissions match the release layout.
    ReleaseLayout,
    /// Operator configuration and runtime state remain protected from package ownership.
    ProtectedRoots,
    /// Native login supervisor starts only the exact foreground G0 command.
    ServiceAutostart,
    /// Crash restart is bounded and orderly stop is not restarted as failure.
    ServiceRestart,
    /// G0 publishes only its same-user Unix socket or Windows named pipe.
    LocalTransport,
    /// A different operating-system user cannot negotiate A3.
    PeerAuthentication,
    /// Packaged G1 obtains authenticated daemon status with stable exits/output.
    CliStatus,
    /// Packaged G2 negotiates, renders, and restores terminal state on shutdown.
    TuiLifecycle,
    /// Packaged processes match release-control argv, streams, exits, and protocol observations.
    ProcessEquivalence,
    /// Pipe mode preserves distinct bounded stdout and stderr.
    PipeSeparation,
    /// PTY/ConPTY mode preserves ordered output, control, exit, and cleanup.
    TerminalOwnership,
    /// Cancellation terminates and reaps the complete owned process tree.
    CancellationTreeReap,
    /// Restricted filesystem/network denial is native and has no raw fallback.
    SandboxDenial,
    /// Admitted native restricted execution binds helper/probe/plan and completes release.
    SandboxExecution,
    /// Package upgrade retains configuration, durable state, logs, and endpoint identity.
    UpgradePreservation,
    /// Failed upgrade restores the prior package while retaining protected roots.
    UpgradeRollback,
    /// Ordinary uninstall removes package-owned files and preserves operator/runtime roots.
    UninstallPreservation,
}

/// Declarative scenario contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ScenarioSpec {
    id: ScenarioId,
    category: ScenarioCategory,
    required: bool,
    description: &'static str,
}

impl ScenarioSpec {
    /// Returns the stable identity.
    #[must_use]
    pub const fn id(self) -> ScenarioId {
        self.id
    }

    /// Returns the scenario family.
    #[must_use]
    pub const fn category(self) -> ScenarioCategory {
        self.category
    }

    /// Reports whether an unsupported result makes the target not ready.
    #[must_use]
    pub const fn required(self) -> bool {
        self.required
    }

    /// Returns the stable contract description.
    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }
}

impl ScenarioId {
    /// Returns the complete canonical H2 scenario list.
    #[must_use]
    pub const fn all() -> &'static [ScenarioSpec] {
        &SCENARIOS
    }
}

const SCENARIOS: [ScenarioSpec; 18] = [
    spec(
        ScenarioId::ArtifactIntegrity,
        ScenarioCategory::Package,
        "verify canonical manifest and artifact checksums",
    ),
    spec(
        ScenarioId::ReleaseLayout,
        ScenarioCategory::Package,
        "verify installed paths, types, owners, and permissions",
    ),
    spec(
        ScenarioId::ProtectedRoots,
        ScenarioCategory::Package,
        "verify operator and runtime roots are not package owned",
    ),
    spec(
        ScenarioId::ServiceAutostart,
        ScenarioCategory::Service,
        "verify per-user login autostart and exact foreground argv",
    ),
    spec(
        ScenarioId::ServiceRestart,
        ScenarioCategory::Service,
        "verify bounded crash restart and clean-stop behavior",
    ),
    spec(
        ScenarioId::LocalTransport,
        ScenarioCategory::Service,
        "verify the single local endpoint and absence of remote listeners",
    ),
    spec(
        ScenarioId::PeerAuthentication,
        ScenarioCategory::Service,
        "verify same-user operating-system peer authentication",
    ),
    spec(
        ScenarioId::CliStatus,
        ScenarioCategory::Application,
        "verify packaged CLI status and stable process output",
    ),
    spec(
        ScenarioId::TuiLifecycle,
        ScenarioCategory::Application,
        "verify packaged TUI negotiation and terminal restoration",
    ),
    spec(
        ScenarioId::ProcessEquivalence,
        ScenarioCategory::Application,
        "compare packaged binaries with release controls",
    ),
    spec(
        ScenarioId::PipeSeparation,
        ScenarioCategory::Runtime,
        "verify bounded distinct stdout and stderr",
    ),
    spec(
        ScenarioId::TerminalOwnership,
        ScenarioCategory::Runtime,
        "verify PTY or ConPTY ownership and ordering",
    ),
    spec(
        ScenarioId::CancellationTreeReap,
        ScenarioCategory::Runtime,
        "verify full-tree cancellation and reap",
    ),
    spec(
        ScenarioId::SandboxDenial,
        ScenarioCategory::Runtime,
        "verify native denial without raw fallback",
    ),
    spec(
        ScenarioId::SandboxExecution,
        ScenarioCategory::Runtime,
        "verify admitted restricted execution and release",
    ),
    spec(
        ScenarioId::UpgradePreservation,
        ScenarioCategory::Lifecycle,
        "verify protected roots survive upgrade",
    ),
    spec(
        ScenarioId::UpgradeRollback,
        ScenarioCategory::Lifecycle,
        "verify failed upgrade restores prior package",
    ),
    spec(
        ScenarioId::UninstallPreservation,
        ScenarioCategory::Lifecycle,
        "verify uninstall ownership and preservation",
    ),
];

const fn spec(
    id: ScenarioId,
    category: ScenarioCategory,
    description: &'static str,
) -> ScenarioSpec {
    ScenarioSpec { id, category, required: true, description }
}
