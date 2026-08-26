//! Compiled harness ceilings and manifest-controlled tightening.

use peritus_patch::{MAX_FILE_BYTES, MAX_PATCH_BYTES, MAX_PATCH_OPERATIONS};

use crate::domain::{HarnessDomainError, HarnessDomainErrorKind, HarnessLimitKind};

/// Complete set of resource bounds applied by pure domain constructors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HarnessLimits {
    manifest_bytes: u64,
    components: u64,
    dependency_edges: u64,
    dependency_fan_out: u64,
    component_bytes: u64,
    total_materialized_bytes: u64,
    revision_history: u64,
    receipt_history: u64,
    event_bytes: u64,
    state_bytes: u64,
    retained_diagnostics: u64,
}

impl HarnessLimits {
    /// Production ceilings that a manifest may tighten but never widen.
    pub const COMPILED: Self = Self {
        manifest_bytes: 4 * 1_024 * 1_024,
        components: MAX_PATCH_OPERATIONS as u64,
        dependency_edges: 32_768,
        dependency_fan_out: 256,
        component_bytes: MAX_FILE_BYTES as u64,
        total_materialized_bytes: MAX_PATCH_BYTES as u64,
        revision_history: 10_000,
        receipt_history: 10_000,
        event_bytes: 16 * 1_024 * 1_024,
        state_bytes: 16 * 1_024 * 1_024,
        retained_diagnostics: 1_024,
    };

    /// Returns the production compiled ceilings.
    #[must_use]
    pub const fn compiled() -> Self {
        Self::COMPILED
    }

    /// Returns a copy with one bound tightened.
    ///
    /// # Errors
    ///
    /// Rejects zero or a value greater than the corresponding compiled ceiling.
    pub const fn tighten(
        self,
        kind: HarnessLimitKind,
        value: u64,
    ) -> Result<Self, HarnessDomainError> {
        if value == 0 {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::InvalidLimit,
                kind,
                1,
                value,
            ));
        }
        let ceiling = self.value(kind);
        if value > ceiling {
            return Err(HarnessDomainError::limit(
                HarnessDomainErrorKind::LimitWidening,
                kind,
                ceiling,
                value,
            ));
        }
        let mut tightened = self;
        tightened.set(kind, value);
        Ok(tightened)
    }

    /// Applies ordered manifest overrides, rejecting every attempted widening.
    ///
    /// # Errors
    ///
    /// Returns the first invalid or widening limit diagnostic.
    pub fn tightened(
        mut self,
        overrides: &[(HarnessLimitKind, u64)],
    ) -> Result<Self, HarnessDomainError> {
        for &(kind, value) in overrides {
            self = self.tighten(kind, value)?;
        }
        Ok(self)
    }

    /// Returns whether every bound is no wider than `ceiling`.
    #[must_use]
    pub fn is_within(self, ceiling: Self) -> bool {
        LIMIT_KINDS.into_iter().all(|kind| self.value(kind) <= ceiling.value(kind))
    }

    /// Returns the value of one named limit.
    #[must_use]
    pub const fn value(self, kind: HarnessLimitKind) -> u64 {
        match kind {
            HarnessLimitKind::ManifestBytes => self.manifest_bytes,
            HarnessLimitKind::Components => self.components,
            HarnessLimitKind::DependencyEdges => self.dependency_edges,
            HarnessLimitKind::DependencyFanOut => self.dependency_fan_out,
            HarnessLimitKind::ComponentBytes => self.component_bytes,
            HarnessLimitKind::TotalMaterializedBytes => self.total_materialized_bytes,
            HarnessLimitKind::RevisionHistory => self.revision_history,
            HarnessLimitKind::ReceiptHistory => self.receipt_history,
            HarnessLimitKind::EventBytes => self.event_bytes,
            HarnessLimitKind::StateBytes => self.state_bytes,
            HarnessLimitKind::RetainedDiagnostics => self.retained_diagnostics,
        }
    }

    const fn set(&mut self, kind: HarnessLimitKind, value: u64) {
        match kind {
            HarnessLimitKind::ManifestBytes => self.manifest_bytes = value,
            HarnessLimitKind::Components => self.components = value,
            HarnessLimitKind::DependencyEdges => self.dependency_edges = value,
            HarnessLimitKind::DependencyFanOut => self.dependency_fan_out = value,
            HarnessLimitKind::ComponentBytes => self.component_bytes = value,
            HarnessLimitKind::TotalMaterializedBytes => self.total_materialized_bytes = value,
            HarnessLimitKind::RevisionHistory => self.revision_history = value,
            HarnessLimitKind::ReceiptHistory => self.receipt_history = value,
            HarnessLimitKind::EventBytes => self.event_bytes = value,
            HarnessLimitKind::StateBytes => self.state_bytes = value,
            HarnessLimitKind::RetainedDiagnostics => self.retained_diagnostics = value,
        }
    }

    /// Maximum manifest byte count.
    #[must_use]
    pub const fn max_manifest_bytes(self) -> u64 {
        self.manifest_bytes
    }
    /// Maximum component count.
    #[must_use]
    pub const fn max_components(self) -> u64 {
        self.components
    }
    /// Maximum total dependency edge count.
    #[must_use]
    pub const fn max_dependency_edges(self) -> u64 {
        self.dependency_edges
    }
    /// Maximum dependency fan-out of one component.
    #[must_use]
    pub const fn max_dependency_fan_out(self) -> u64 {
        self.dependency_fan_out
    }
    /// Maximum byte count of one component.
    #[must_use]
    pub const fn max_component_bytes(self) -> u64 {
        self.component_bytes
    }
    /// Maximum aggregate materialized byte count.
    #[must_use]
    pub const fn max_total_materialized_bytes(self) -> u64 {
        self.total_materialized_bytes
    }
    /// Maximum retained revision count.
    #[must_use]
    pub const fn max_revision_history(self) -> u64 {
        self.revision_history
    }
    /// Maximum retained receipt count.
    #[must_use]
    pub const fn max_receipt_history(self) -> u64 {
        self.receipt_history
    }
    /// Maximum encoded event byte count.
    #[must_use]
    pub const fn max_event_bytes(self) -> u64 {
        self.event_bytes
    }
    /// Maximum encoded aggregate-state byte count.
    #[must_use]
    pub const fn max_state_bytes(self) -> u64 {
        self.state_bytes
    }
    /// Maximum retained diagnostic count.
    #[must_use]
    pub const fn max_retained_diagnostics(self) -> u64 {
        self.retained_diagnostics
    }
}

impl Default for HarnessLimits {
    fn default() -> Self {
        Self::compiled()
    }
}

const LIMIT_KINDS: [HarnessLimitKind; 11] = [
    HarnessLimitKind::ManifestBytes,
    HarnessLimitKind::Components,
    HarnessLimitKind::DependencyEdges,
    HarnessLimitKind::DependencyFanOut,
    HarnessLimitKind::ComponentBytes,
    HarnessLimitKind::TotalMaterializedBytes,
    HarnessLimitKind::RevisionHistory,
    HarnessLimitKind::ReceiptHistory,
    HarnessLimitKind::EventBytes,
    HarnessLimitKind::StateBytes,
    HarnessLimitKind::RetainedDiagnostics,
];
