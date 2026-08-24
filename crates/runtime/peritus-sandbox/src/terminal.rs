//! Pipe and pseudo-terminal contracts.

use crate::SandboxError;
use peritus_types::ResourceQuantity;

/// Lifecycle observations retained even when optional observation capacity is exhausted.
pub const REQUIRED_LIFECYCLE_EVENTS: u64 = 5;

/// Available terminal transport modes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalMode {
    /// Separate standard streams connected to pipes.
    Pipes,
    /// A pseudo-terminal session.
    Pty,
}

impl TerminalMode {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Pipes => 0,
            Self::Pty => 1,
        }
    }
}

/// Compact set of terminal modes.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerminalModes(u8);

impl TerminalModes {
    /// Returns no supported modes.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }
    /// Returns a set containing the supplied modes.
    #[must_use]
    pub fn from_modes(modes: impl IntoIterator<Item = TerminalMode>) -> Self {
        let mut result = Self::empty();
        for mode in modes {
            result.insert(mode);
        }
        result
    }
    /// Adds a mode.
    pub const fn insert(&mut self, mode: TerminalMode) {
        self.0 |= match mode {
            TerminalMode::Pipes => 1,
            TerminalMode::Pty => 2,
        };
    }
    /// Reports whether a mode is present.
    #[must_use]
    pub const fn contains(self, mode: TerminalMode) -> bool {
        self.0
            & match mode {
                TerminalMode::Pipes => 1,
                TerminalMode::Pty => 2,
            }
            != 0
    }
    /// Returns the stable bit representation.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Input forwarding policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InputPermission {
    /// Input is disabled.
    Denied,
    /// Input may be forwarded.
    Allowed,
}

impl InputPermission {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Denied => 0,
            Self::Allowed => 1,
        }
    }
}
/// Terminal resizing policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResizePermission {
    /// Resizing is disabled.
    Denied,
    /// Resizing may be requested.
    Allowed,
}

impl ResizePermission {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Denied => 0,
            Self::Allowed => 1,
        }
    }
}
/// Terminal signal forwarding policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalSignalPermission {
    /// Signal forwarding is disabled.
    Denied,
    /// Signal forwarding is permitted.
    Allowed,
}

impl TerminalSignalPermission {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Denied => 0,
            Self::Allowed => 1,
        }
    }
}

/// Validated terminal dimensions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerminalSize {
    columns: u16,
    rows: u16,
}

impl TerminalSize {
    /// Creates nonzero dimensions.
    ///
    /// # Errors
    /// Rejects zero columns or rows.
    pub const fn new(columns: u16, rows: u16) -> Result<Self, SandboxError> {
        if columns == 0 || rows == 0 {
            return Err(crate::error::invalid("terminal dimensions must be nonzero"));
        }
        Ok(Self { columns, rows })
    }
    /// Returns columns.
    #[must_use]
    pub const fn columns(self) -> u16 {
        self.columns
    }
    /// Returns rows.
    #[must_use]
    pub const fn rows(self) -> u16 {
        self.rows
    }
}

/// Terminal initial-dimension, event-count, and output-byte bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalLimits {
    maximum_initial_size: Option<TerminalSize>,
    event_count: ResourceQuantity,
    output_bytes: ResourceQuantity,
}

impl TerminalLimits {
    /// Creates complete terminal bounds.
    ///
    /// # Errors
    /// Rejects fewer than five lifecycle events or a zero output-byte bound.
    pub const fn new(
        maximum_initial_size: Option<TerminalSize>,
        event_count: ResourceQuantity,
        output_bytes: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        if event_count.get() < REQUIRED_LIFECYCLE_EVENTS {
            return Err(crate::error::invalid("terminal event bound cannot retain lifecycle"));
        }
        if output_bytes.get() == 0 {
            return Err(crate::error::invalid("terminal output bound must be nonzero"));
        }
        Ok(Self { maximum_initial_size, event_count, output_bytes })
    }

    /// Returns the maximum allowed initial PTY dimensions.
    #[must_use]
    pub const fn maximum_initial_size(self) -> Option<TerminalSize> {
        self.maximum_initial_size
    }
    /// Returns the maximum retained terminal/sandbox event count.
    #[must_use]
    pub const fn event_count(self) -> ResourceQuantity {
        self.event_count
    }
    /// Returns the maximum terminal output bytes.
    #[must_use]
    pub const fn output_bytes(self) -> ResourceQuantity {
        self.output_bytes
    }
}

/// Terminal capability contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalContract {
    modes: TerminalModes,
    input: InputPermission,
    resize: ResizePermission,
    signals: TerminalSignalPermission,
    limits: TerminalLimits,
}

impl TerminalContract {
    /// Creates a terminal contract.
    ///
    /// # Errors
    /// Rejects PTY dimensions or resize capability without PTY support, and requires maximum
    /// initial dimensions whenever PTY mode is supported.
    pub const fn new(
        modes: TerminalModes,
        input: InputPermission,
        resize: ResizePermission,
        signals: TerminalSignalPermission,
        limits: TerminalLimits,
    ) -> Result<Self, SandboxError> {
        let has_pty = modes.contains(TerminalMode::Pty);
        if has_pty != limits.maximum_initial_size().is_some() {
            return Err(crate::error::invalid("PTY mode and maximum dimensions must agree"));
        }
        if matches!(resize, ResizePermission::Allowed) && !has_pty {
            return Err(crate::error::invalid("terminal resize requires PTY mode"));
        }
        Ok(Self { modes, input, resize, signals, limits })
    }
    /// Returns supported modes.
    #[must_use]
    pub const fn modes(self) -> TerminalModes {
        self.modes
    }
    /// Returns input policy.
    #[must_use]
    pub const fn input(self) -> InputPermission {
        self.input
    }
    /// Returns resize policy.
    #[must_use]
    pub const fn resize(self) -> ResizePermission {
        self.resize
    }
    /// Returns signal policy.
    #[must_use]
    pub const fn signals(self) -> TerminalSignalPermission {
        self.signals
    }
    /// Returns terminal bounds.
    #[must_use]
    pub const fn limits(self) -> TerminalLimits {
        self.limits
    }
}

/// Terminal behavior required by an invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalRequirements {
    mode: TerminalMode,
    input: InputPermission,
    resize: ResizePermission,
    signals: TerminalSignalPermission,
    initial_size: Option<TerminalSize>,
    event_count: ResourceQuantity,
    output_bytes: ResourceQuantity,
}

impl TerminalRequirements {
    /// Creates terminal requirements.
    ///
    /// # Errors
    /// Rejects invalid pipe/PTY dimension or resize combinations, fewer than five events, and a
    /// zero output-byte request.
    pub const fn new(
        mode: TerminalMode,
        input: InputPermission,
        resize: ResizePermission,
        signals: TerminalSignalPermission,
        initial_size: Option<TerminalSize>,
        event_count: ResourceQuantity,
        output_bytes: ResourceQuantity,
    ) -> Result<Self, SandboxError> {
        if matches!(mode, TerminalMode::Pipes) && initial_size.is_some() {
            return Err(crate::error::invalid("initial terminal dimensions require PTY mode"));
        }
        if matches!(mode, TerminalMode::Pipes) && matches!(resize, ResizePermission::Allowed) {
            return Err(crate::error::invalid("terminal resize requires PTY mode"));
        }
        if event_count.get() < REQUIRED_LIFECYCLE_EVENTS || output_bytes.get() == 0 {
            return Err(crate::error::invalid("invalid terminal event or output requirement"));
        }
        Ok(Self { mode, input, resize, signals, initial_size, event_count, output_bytes })
    }
    /// Returns the required mode.
    #[must_use]
    pub const fn mode(self) -> TerminalMode {
        self.mode
    }
    /// Returns input requirement.
    #[must_use]
    pub const fn input(self) -> InputPermission {
        self.input
    }
    /// Returns resize requirement.
    #[must_use]
    pub const fn resize(self) -> ResizePermission {
        self.resize
    }
    /// Returns signal requirement.
    #[must_use]
    pub const fn signals(self) -> TerminalSignalPermission {
        self.signals
    }
    /// Returns initial size.
    #[must_use]
    pub const fn initial_size(self) -> Option<TerminalSize> {
        self.initial_size
    }
    /// Returns the requested event bound.
    #[must_use]
    pub const fn event_count(self) -> ResourceQuantity {
        self.event_count
    }
    /// Returns the requested terminal output bound.
    #[must_use]
    pub const fn output_bytes(self) -> ResourceQuantity {
        self.output_bytes
    }

    pub(crate) const fn is_allowed_by(self, contract: TerminalContract) -> bool {
        contract.modes.contains(self.mode)
            && !(matches!(self.input, InputPermission::Allowed)
                && matches!(contract.input, InputPermission::Denied))
            && !(matches!(self.resize, ResizePermission::Allowed)
                && matches!(contract.resize, ResizePermission::Denied))
            && !(matches!(self.signals, TerminalSignalPermission::Allowed)
                && matches!(contract.signals, TerminalSignalPermission::Denied))
            && self.event_count.get() <= contract.limits.event_count.get()
            && self.output_bytes.get() <= contract.limits.output_bytes.get()
            && match (self.initial_size, contract.limits.maximum_initial_size) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(requested), Some(maximum)) => {
                    requested.columns <= maximum.columns && requested.rows <= maximum.rows
                }
            }
    }
}

/// A runtime terminal operation used by reference probes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestedTerminalOperation {
    /// Forward input.
    Input,
    /// Resize the terminal.
    Resize(TerminalSize),
    /// Forward a terminal signal.
    Signal,
}
