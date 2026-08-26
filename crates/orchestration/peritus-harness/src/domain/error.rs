//! Stable failures emitted by checked harness-domain constructors.

use crate::domain::ComponentId;

/// Stable category for a harness-domain rejection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HarnessDomainErrorKind {
    /// A required textual value was empty.
    EmptyValue,
    /// A textual value exceeded its compiled bound.
    ValueTooLong,
    /// A textual value contained a forbidden character or shape.
    InvalidValue,
    /// A path was not normalized or escaped its permitted root.
    InvalidPath,
    /// A target named a workspace control path.
    ProtectedPath,
    /// A schema version was zero.
    InvalidSchemaVersion,
    /// A schema interval was empty.
    InvalidSchemaInterval,
    /// A set or ordered list was not in strict canonical order.
    NonCanonicalOrder,
    /// A configured limit was zero.
    InvalidLimit,
    /// A requested limit exceeded the compiled ceiling.
    LimitWidening,
    /// A component count exceeded its bound.
    TooManyComponents,
    /// A component's dependency fan-out exceeded its bound.
    TooManyDependencies,
    /// Total dependency edges exceeded their bound.
    TooManyDependencyEdges,
    /// One component exceeded its declared or compiled byte bound.
    ComponentTooLarge,
    /// Aggregate materialized bytes exceeded their bound.
    TotalBytesExceeded,
    /// A declaration supplied a protection class different from compiled policy.
    ProtectionMismatch,
    /// A declaration's schema was outside its own compatibility interval.
    CompatibilityMismatch,
    /// Two declarations used one stable component identity.
    DuplicateComponent,
    /// Two declarations used one source path.
    DuplicateSourcePath,
    /// Two declarations used one target path.
    DuplicateTargetPath,
    /// Two target files collide as ancestor and descendant paths.
    TargetPathCollision,
    /// A declaration listed one dependency more than once.
    DuplicateDependency,
    /// A dependency named the component that declared it.
    SelfDependency,
    /// A dependency identity was absent.
    MissingDependency,
    /// The dependency graph contained a cycle.
    DependencyCycle,
    /// A dependency had a different component kind.
    IncompatibleDependencyKind,
    /// A dependency schema version was outside the required interval.
    IncompatibleDependencyVersion,
    /// A dependency's exact digest requirement disagreed.
    DependencyDigestMismatch,
    /// A provider feature requirement was unavailable.
    UnsatisfiedProviderFeature,
    /// A platform feature requirement was unavailable.
    UnsatisfiedPlatformFeature,
    /// Declared authority exceeded the kind's compiled ceiling.
    AuthorityExceeded,
    /// Dependency-closure authority exceeded the depender's compiled ceiling.
    DependencyAuthorityExceeded,
    /// An evolvable component depended on an incompatible protected asset.
    ProtectedDependency,
    /// Verified content omitted a declared component.
    MissingContent,
    /// Verified content repeated a component identity.
    DuplicateContent,
    /// Verified content named an undeclared component.
    UnexpectedContent,
    /// Supplied bytes disagreed with the declaration's byte length.
    ContentLengthMismatch,
    /// Supplied bytes disagreed with the declaration's SHA-256 digest.
    ContentDigestMismatch,
    /// Checked integer arithmetic overflowed.
    ArithmeticOverflow,
    /// A revision number could not advance.
    RevisionOverflow,
    /// A successor attempted to alter a protected asset.
    ProtectedAssetDrift,
    /// A revision used a different harness lineage identity.
    HarnessIdentityMismatch,
    /// A successor did not name its exact predecessor digest.
    PredecessorMismatch,
    /// A successor did not increment its predecessor number exactly once.
    RevisionNumberMismatch,
    /// History already contained the full revision digest.
    DuplicateRevision,
    /// History already contained a different genesis revision.
    GenesisConflict,
    /// A successor's direct predecessor was absent from history.
    OrphanRevision,
    /// Revision history exceeded its compiled or tightened bound.
    HistoryLimitExceeded,
    /// A rollback source was absent from history.
    RollbackSourceMissing,
    /// A rollback target was absent from history.
    RollbackTargetMissing,
    /// A rollback target was not an ancestor of its selected source.
    RollbackNotAncestor,
    /// Canonical bytes were malformed, noncanonical, or had trailing data.
    InvalidCanonicalEncoding,
    /// A canonical digest did not bind the decoded value.
    CanonicalDigestMismatch,
}

/// Limit named by a structured domain diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HarnessLimitKind {
    /// Manifest byte count.
    ManifestBytes,
    /// Component count.
    Components,
    /// Total dependency edge count.
    DependencyEdges,
    /// Per-component dependency fan-out.
    DependencyFanOut,
    /// Per-component byte count.
    ComponentBytes,
    /// Aggregate materialized byte count.
    TotalMaterializedBytes,
    /// Retained revision count.
    RevisionHistory,
    /// Retained receipt count.
    ReceiptHistory,
    /// Encoded event bytes.
    EventBytes,
    /// Encoded aggregate-state bytes.
    StateBytes,
    /// Retained diagnostic count.
    RetainedDiagnostics,
}

/// Comparable error carrying precise optional component, value, and bound context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessDomainError {
    kind: HarnessDomainErrorKind,
    component_id: Option<ComponentId>,
    related_id: Option<ComponentId>,
    limit: Option<HarnessLimitKind>,
    expected: Option<u64>,
    actual: Option<u64>,
    detail: Option<String>,
}

impl HarnessDomainError {
    pub(crate) const fn plain(kind: HarnessDomainErrorKind) -> Self {
        Self::new(kind, None, None, None, None, None, None)
    }

    pub(crate) fn detail(kind: HarnessDomainErrorKind, detail: impl Into<String>) -> Self {
        Self::new(kind, None, None, None, None, None, Some(detail.into()))
    }

    pub(crate) const fn component(kind: HarnessDomainErrorKind, component_id: ComponentId) -> Self {
        Self::new(kind, Some(component_id), None, None, None, None, None)
    }

    pub(crate) const fn components(
        kind: HarnessDomainErrorKind,
        component_id: ComponentId,
        related_id: ComponentId,
    ) -> Self {
        Self::new(kind, Some(component_id), Some(related_id), None, None, None, None)
    }

    pub(crate) fn component_detail(
        kind: HarnessDomainErrorKind,
        component_id: ComponentId,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(kind, Some(component_id), None, None, None, None, Some(detail.into()))
    }

    pub(crate) const fn limit(
        kind: HarnessDomainErrorKind,
        limit: HarnessLimitKind,
        expected: u64,
        actual: u64,
    ) -> Self {
        Self::new(kind, None, None, Some(limit), Some(expected), Some(actual), None)
    }

    pub(crate) const fn component_numbers(
        kind: HarnessDomainErrorKind,
        component_id: ComponentId,
        expected: u64,
        actual: u64,
    ) -> Self {
        Self::new(kind, Some(component_id), None, None, Some(expected), Some(actual), None)
    }

    const fn new(
        kind: HarnessDomainErrorKind,
        component_id: Option<ComponentId>,
        related_id: Option<ComponentId>,
        limit: Option<HarnessLimitKind>,
        expected: Option<u64>,
        actual: Option<u64>,
        detail: Option<String>,
    ) -> Self {
        Self { kind, component_id, related_id, limit, expected, actual, detail }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> HarnessDomainErrorKind {
        self.kind
    }

    /// Returns the primary affected component, when present.
    #[must_use]
    pub const fn component_id(&self) -> Option<&ComponentId> {
        self.component_id.as_ref()
    }

    /// Returns a related dependency component, when present.
    #[must_use]
    pub const fn related_id(&self) -> Option<&ComponentId> {
        self.related_id.as_ref()
    }

    /// Returns the affected limit, when present.
    #[must_use]
    pub const fn limit_kind(&self) -> Option<HarnessLimitKind> {
        self.limit
    }

    /// Returns the expected value or ceiling, when present.
    #[must_use]
    pub const fn expected(&self) -> Option<u64> {
        self.expected
    }

    /// Returns the observed value, when present.
    #[must_use]
    pub const fn actual(&self) -> Option<u64> {
        self.actual
    }

    /// Returns bounded diagnostic text, when present.
    #[must_use]
    pub fn detail_text(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

impl std::fmt::Display for HarnessDomainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "harness domain rejected input: {:?}", self.kind)?;
        if let Some(component_id) = &self.component_id {
            write!(formatter, " for component {component_id}")?;
        }
        if let Some(detail) = &self.detail {
            write!(formatter, ": {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for HarnessDomainError {}
