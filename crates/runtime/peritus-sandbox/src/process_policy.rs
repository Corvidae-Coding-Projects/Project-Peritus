//! Process-tree capability contracts.

use crate::{SandboxError, SandboxPath};

/// Descendant creation policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DescendantPolicy {
    /// No descendant may be created.
    Denied,
    /// At most the stated number of descendants may exist.
    Bounded(u32),
    /// Descendant creation is allowed without a synthetic cumulative count.
    Allowed,
}

/// Signal delivery policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SignalPolicy {
    /// Do not permit supervisor-delivered signals.
    Denied,
    /// Permit graceful termination only.
    GracefulOnly,
    /// Permit graceful and forced termination.
    GracefulAndForced,
}

impl SignalPolicy {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Denied => 0,
            Self::GracefulOnly => 1,
            Self::GracefulAndForced => 2,
        }
    }
}

/// Required containment of the owned process tree.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TreeContainment {
    /// The backend must contain the complete descendant tree.
    Required,
    /// Native containment is not required for an explicitly raw effect.
    NotRequiredForRawEffect,
}

impl TreeContainment {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Required => 0,
            Self::NotRequiredForRawEffect => 1,
        }
    }
}

/// Canonical process execution contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessContract {
    root_programs: Vec<SandboxPath>,
    descendants: DescendantPolicy,
    signals: SignalPolicy,
    containment: TreeContainment,
    maximum_processes: Option<u32>,
}

impl ProcessContract {
    /// Creates a process contract.
    ///
    /// # Errors
    /// Rejects an empty root set or a zero/inconsistent process bound.
    pub fn new(
        root_programs: Vec<SandboxPath>,
        descendants: DescendantPolicy,
        signals: SignalPolicy,
        containment: TreeContainment,
        maximum_processes: u32,
    ) -> Result<Self, SandboxError> {
        Self::with_optional_process_limit(
            root_programs,
            descendants,
            signals,
            containment,
            Some(maximum_processes),
        )
    }

    /// Creates a process contract with an explicit optional simultaneous-process ceiling.
    ///
    /// # Errors
    /// Rejects an empty root set, a selected zero/inconsistent process ceiling, or an
    /// unbounded descendant policy paired with a finite process ceiling.
    pub fn with_optional_process_limit(
        mut root_programs: Vec<SandboxPath>,
        descendants: DescendantPolicy,
        signals: SignalPolicy,
        containment: TreeContainment,
        maximum_processes: Option<u32>,
    ) -> Result<Self, SandboxError> {
        root_programs.sort();
        root_programs.dedup();
        if root_programs.is_empty() {
            return Err(crate::error::bound("invalid root program count"));
        }
        if maximum_processes == Some(0) {
            return Err(crate::error::invalid("selected process limit must be nonzero"));
        }
        let needed = match descendants {
            DescendantPolicy::Denied => Some(1),
            DescendantPolicy::Bounded(n) => n.checked_add(1),
            DescendantPolicy::Allowed => None,
        };
        if matches!((needed, maximum_processes), (Some(required), Some(maximum)) if maximum < required)
            || needed.is_some() != maximum_processes.is_some()
        {
            return Err(crate::error::invalid("process limit is below descendant allowance"));
        }
        Ok(Self { root_programs, descendants, signals, containment, maximum_processes })
    }

    /// Returns canonical allowed root programs.
    #[must_use]
    pub fn root_programs(&self) -> &[SandboxPath] {
        &self.root_programs
    }
    /// Returns descendant policy.
    #[must_use]
    pub const fn descendants(&self) -> DescendantPolicy {
        self.descendants
    }
    /// Returns signal policy.
    #[must_use]
    pub const fn signals(&self) -> SignalPolicy {
        self.signals
    }
    /// Returns containment requirement.
    #[must_use]
    pub const fn containment(&self) -> TreeContainment {
        self.containment
    }
    /// Returns maximum simultaneous owned processes, including the root.
    #[must_use]
    pub const fn maximum_processes(&self) -> u32 {
        match self.maximum_processes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional simultaneous owned-process ceiling, including the root.
    #[must_use]
    pub const fn maximum_process_limit(&self) -> Option<u32> {
        self.maximum_processes
    }

    pub(crate) const fn uses_legacy_encoding(&self) -> bool {
        !matches!(self.descendants, DescendantPolicy::Allowed)
            && self.maximum_processes.is_some()
    }
}

/// Process behavior required by a planned invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessRequirements {
    program: SandboxPath,
    descendant_count: u32,
    requires_forced_termination: bool,
}

impl ProcessRequirements {
    /// Creates process requirements.
    #[must_use]
    pub const fn new(
        program: SandboxPath,
        descendant_count: u32,
        requires_forced_termination: bool,
    ) -> Self {
        Self { program, descendant_count, requires_forced_termination }
    }

    /// Returns the root program.
    #[must_use]
    pub const fn program(&self) -> &SandboxPath {
        &self.program
    }
    /// Returns the greatest required descendant count.
    #[must_use]
    pub const fn descendant_count(&self) -> u32 {
        self.descendant_count
    }
    /// Reports whether forced termination is required.
    #[must_use]
    pub const fn requires_forced_termination(&self) -> bool {
        self.requires_forced_termination
    }

    pub(crate) fn is_allowed_by(&self, contract: &ProcessContract) -> bool {
        contract.root_programs.contains(&self.program)
            && match contract.descendants {
                DescendantPolicy::Denied => self.descendant_count == 0,
                DescendantPolicy::Bounded(limit) => self.descendant_count <= limit,
                DescendantPolicy::Allowed => true,
            }
            && (!self.requires_forced_termination
                || contract.signals == SignalPolicy::GracefulAndForced)
    }
}
