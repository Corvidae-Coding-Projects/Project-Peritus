//! Closed backend enforcement-feature vocabulary.

/// One independently required sandbox enforcement feature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SandboxFeature {
    /// Filesystem path discovery control.
    FilesystemDiscover,
    /// Filesystem metadata control.
    FilesystemMetadata,
    /// Filesystem read control.
    FilesystemRead,
    /// Filesystem execute control.
    FilesystemExecute,
    /// Filesystem create control.
    FilesystemCreate,
    /// Filesystem write control.
    FilesystemWrite,
    /// Filesystem remove control.
    FilesystemRemove,
    /// Root executable control.
    ProcessRoot,
    /// Descendant-process control.
    ProcessDescendants,
    /// Process signal/control mediation.
    ProcessSignals,
    /// Complete process-tree containment.
    ProcessTree,
    /// Cleared child environment.
    EnvironmentClear,
    /// Explicit environment allowlist.
    EnvironmentAllowList,
    /// Default network denial.
    NetworkDeny,
    /// Exact outbound network allow rules.
    NetworkEgress,
    /// Secret delivery through environment.
    SecretEnvironment,
    /// Secret delivery through a file.
    SecretFile,
    /// Secret delivery through a brokered handle.
    SecretHandle,
    /// Wall-time enforcement.
    WallTime,
    /// CPU-time enforcement or exact reference accounting.
    CpuTime,
    /// Memory enforcement or exact reference accounting.
    Memory,
    /// Disk enforcement or exact reference accounting.
    Disk,
    /// Output enforcement.
    Output,
    /// File-descriptor or handle enforcement.
    OpenHandles,
    /// Owned process-count enforcement.
    ProcessCount,
    /// Concurrent-slot enforcement.
    Concurrency,
    /// Pipe execution.
    Pipes,
    /// Pseudoterminal execution.
    Pty,
    /// Child standard input.
    Stdin,
    /// Terminal resize.
    TerminalResize,
    /// Terminal signal delivery.
    TerminalSignals,
}

impl SandboxFeature {
    pub(crate) const ALL: [Self; 31] = [
        Self::FilesystemDiscover,
        Self::FilesystemMetadata,
        Self::FilesystemRead,
        Self::FilesystemExecute,
        Self::FilesystemCreate,
        Self::FilesystemWrite,
        Self::FilesystemRemove,
        Self::ProcessRoot,
        Self::ProcessDescendants,
        Self::ProcessSignals,
        Self::ProcessTree,
        Self::EnvironmentClear,
        Self::EnvironmentAllowList,
        Self::NetworkDeny,
        Self::NetworkEgress,
        Self::SecretEnvironment,
        Self::SecretFile,
        Self::SecretHandle,
        Self::WallTime,
        Self::CpuTime,
        Self::Memory,
        Self::Disk,
        Self::Output,
        Self::OpenHandles,
        Self::ProcessCount,
        Self::Concurrency,
        Self::Pipes,
        Self::Pty,
        Self::Stdin,
        Self::TerminalResize,
        Self::TerminalSignals,
    ];

    pub(crate) const fn bit(self) -> u64 {
        match self {
            Self::FilesystemDiscover => 1 << 0,
            Self::FilesystemMetadata => 1 << 1,
            Self::FilesystemRead => 1 << 2,
            Self::FilesystemExecute => 1 << 3,
            Self::FilesystemCreate => 1 << 4,
            Self::FilesystemWrite => 1 << 5,
            Self::FilesystemRemove => 1 << 6,
            Self::ProcessRoot => 1 << 7,
            Self::ProcessDescendants => 1 << 8,
            Self::ProcessSignals => 1 << 9,
            Self::ProcessTree => 1 << 10,
            Self::EnvironmentClear => 1 << 11,
            Self::EnvironmentAllowList => 1 << 12,
            Self::NetworkDeny => 1 << 13,
            Self::NetworkEgress => 1 << 14,
            Self::SecretEnvironment => 1 << 15,
            Self::SecretFile => 1 << 16,
            Self::SecretHandle => 1 << 17,
            Self::WallTime => 1 << 18,
            Self::CpuTime => 1 << 19,
            Self::Memory => 1 << 20,
            Self::Disk => 1 << 21,
            Self::Output => 1 << 22,
            Self::OpenHandles => 1 << 23,
            Self::ProcessCount => 1 << 24,
            Self::Concurrency => 1 << 25,
            Self::Pipes => 1 << 26,
            Self::Pty => 1 << 27,
            Self::Stdin => 1 << 28,
            Self::TerminalResize => 1 << 29,
            Self::TerminalSignals => 1 << 30,
        }
    }
}

/// Compact canonical set of enforcement features.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureSet(u64);

impl FeatureSet {
    /// Returns the empty feature set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns the complete C2 feature vocabulary.
    #[must_use]
    pub const fn all() -> Self {
        Self((1_u64 << 31) - 1)
    }

    /// Constructs a set from a feature iterator.
    #[must_use]
    pub fn from_features(features: impl IntoIterator<Item = SandboxFeature>) -> Self {
        let mut result = Self::empty();
        for feature in features {
            result.insert(feature);
        }
        result
    }

    /// Returns whether the feature is present.
    #[must_use]
    pub const fn contains(self, feature: SandboxFeature) -> bool {
        self.0 & feature.bit() != 0
    }

    /// Inserts a feature.
    pub const fn insert(&mut self, feature: SandboxFeature) {
        self.0 |= feature.bit();
    }

    /// Returns whether every feature is contained in `supported`.
    #[must_use]
    pub const fn is_subset_of(self, supported: Self) -> bool {
        self.0 & !supported.0 == 0
    }

    /// Returns features present here but absent from `supported`.
    #[must_use]
    pub const fn missing_from(self, supported: Self) -> Self {
        Self(self.0 & !supported.0)
    }

    /// Returns the canonical bit representation.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Returns features in canonical discriminant order.
    #[must_use]
    pub fn iter(self) -> std::vec::IntoIter<SandboxFeature> {
        SandboxFeature::ALL
            .into_iter()
            .filter(|feature| self.contains(*feature))
            .collect::<Vec<_>>()
            .into_iter()
    }
}
