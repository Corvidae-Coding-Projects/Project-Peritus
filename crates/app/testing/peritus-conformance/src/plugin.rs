//! Runtime-neutral G3 plugin manifest, authority, isolation, and lifecycle contract.

mod cases;

pub use cases::plugin_suite;

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginConformanceError {
    /// The plugin boundary could not be exercised or observed.
    Infrastructure,
}

/// One independently exercised extension-host behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginScenario {
    /// Equivalent manifests and artifacts produce stable canonical identities.
    CanonicalManifest,
    /// An artifact without an exact trust anchor cannot start.
    TrustRequired,
    /// Current authority denial occurs before the plugin receives an invocation.
    AuthorityDenied,
    /// A trusted plugin negotiates, invokes, and stops through the versioned protocol.
    Lifecycle,
    /// Host ceilings narrow manifest quotas and reject excess output.
    Quota,
    /// Cancellation terminates and joins the isolated runtime.
    Cancellation,
    /// Plugin failure or exit does not terminate the host process.
    CrashIsolation,
}

/// Stable terminal classification returned by a plugin subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginDisposition {
    /// The requested lifecycle completed successfully.
    Succeeded,
    /// Admission was rejected before a plugin effect.
    Rejected,
    /// The isolated plugin failed explicitly.
    Failed,
    /// Host-driven cancellation completed.
    Cancelled,
}

/// Complete fixed request supplied to one plugin conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PluginConformanceFixture {
    scenario: PluginScenario,
}

impl PluginConformanceFixture {
    pub(crate) const fn new(scenario: PluginScenario) -> Self {
        Self { scenario }
    }

    /// Returns the exact behavior under test.
    #[must_use]
    pub const fn scenario(self) -> PluginScenario {
        self.scenario
    }
}

/// Direct observations from one extension-host exercise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "trust, authority, lifecycle, quota, and isolation facts are independently falsifiable"
)]
pub struct PluginConformanceObservation {
    disposition: PluginDisposition,
    canonical_identity: bool,
    trust_checked: bool,
    authority_checked: bool,
    plugin_effects: u64,
    output_bounded: bool,
    runtime_terminated: bool,
    runtime_joined: bool,
    host_alive: bool,
    truthful_failure: bool,
}

impl PluginConformanceObservation {
    /// Creates complete independently observable plugin-host facts.
    #[must_use]
    #[allow(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        reason = "conformance observations retain separate falsifiable dimensions"
    )]
    pub const fn new(
        disposition: PluginDisposition,
        canonical_identity: bool,
        trust_checked: bool,
        authority_checked: bool,
        plugin_effects: u64,
        output_bounded: bool,
        runtime_terminated: bool,
        runtime_joined: bool,
        host_alive: bool,
        truthful_failure: bool,
    ) -> Self {
        Self {
            disposition,
            canonical_identity,
            trust_checked,
            authority_checked,
            plugin_effects,
            output_bounded,
            runtime_terminated,
            runtime_joined,
            host_alive,
            truthful_failure,
        }
    }

    /// Returns the terminal classification.
    #[must_use]
    pub const fn disposition(self) -> PluginDisposition {
        self.disposition
    }

    /// Returns whether repeated identity derivation was canonical.
    #[must_use]
    pub const fn canonical_identity(self) -> bool {
        self.canonical_identity
    }

    /// Returns whether exact trust was checked.
    #[must_use]
    pub const fn trust_checked(self) -> bool {
        self.trust_checked
    }

    /// Returns whether current authority was checked.
    #[must_use]
    pub const fn authority_checked(self) -> bool {
        self.authority_checked
    }

    /// Returns the exact number of plugin invocation effects observed.
    #[must_use]
    pub const fn plugin_effects(self) -> u64 {
        self.plugin_effects
    }

    /// Returns whether the host enforced its narrowed output ceiling.
    #[must_use]
    pub const fn output_bounded(self) -> bool {
        self.output_bounded
    }

    /// Returns whether the isolated runtime was terminated.
    #[must_use]
    pub const fn runtime_terminated(self) -> bool {
        self.runtime_terminated
    }

    /// Returns whether owned runtime work was joined.
    #[must_use]
    pub const fn runtime_joined(self) -> bool {
        self.runtime_joined
    }

    /// Returns whether the host remained usable after plugin failure.
    #[must_use]
    pub const fn host_alive(self) -> bool {
        self.host_alive
    }

    /// Returns whether non-success was retained as a typed failure.
    #[must_use]
    pub const fn truthful_failure(self) -> bool {
        self.truthful_failure
    }
}

/// Adapter implemented by a production extension host under conformance test.
pub trait PluginConformanceSubject: Send {
    /// Exercises one fixed request and returns direct observations rather than a claimed verdict.
    ///
    /// # Errors
    ///
    /// Returns [`PluginConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &PluginConformanceFixture,
    ) -> Result<PluginConformanceObservation, PluginConformanceError>;
}
