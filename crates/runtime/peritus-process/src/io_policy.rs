//! Checked pipe, PTY, input, output, and deadline policies.

use crate::{ProcessError, error::invalid};

/// Checked terminal dimensions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalSize {
    rows: u16,
    columns: u16,
    pixel_width: u16,
    pixel_height: u16,
}

impl TerminalSize {
    /// Creates nonzero character-cell dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error when rows or columns are zero.
    pub const fn new(
        rows: u16,
        columns: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<Self, ProcessError> {
        if rows == 0 || columns == 0 {
            Err(invalid("terminal rows and columns must be nonzero"))
        } else {
            Ok(Self { rows, columns, pixel_width, pixel_height })
        }
    }

    /// Returns terminal rows.
    #[must_use]
    pub const fn rows(self) -> u16 {
        self.rows
    }
    /// Returns terminal columns.
    #[must_use]
    pub const fn columns(self) -> u16 {
        self.columns
    }
    /// Returns optional pixel width.
    #[must_use]
    pub const fn pixel_width(self) -> u16 {
        self.pixel_width
    }
    /// Returns optional pixel height.
    #[must_use]
    pub const fn pixel_height(self) -> u16 {
        self.pixel_height
    }
}

/// Child standard-I/O arrangement.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IoMode {
    /// Separate stdin, stdout, and stderr pipes.
    Pipes,
    /// One controlling pseudoterminal and combined output stream.
    Pty(TerminalSize),
}

/// Terminal authority and observation bounds projected from the checked sandbox plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalCapabilities {
    resize_allowed: bool,
    signals_allowed: bool,
    event_count: u64,
    output_bytes: Option<u64>,
}

impl TerminalCapabilities {
    pub(crate) const fn new(
        resize_allowed: bool,
        signals_allowed: bool,
        event_count: u64,
        output_bytes: Option<u64>,
    ) -> Self {
        Self { resize_allowed, signals_allowed, event_count, output_bytes }
    }

    /// Returns whether runtime PTY resizing was authorized.
    #[must_use]
    pub const fn resize_allowed(self) -> bool {
        self.resize_allowed
    }

    /// Returns whether portable runtime signal delivery was authorized.
    #[must_use]
    pub const fn signals_allowed(self) -> bool {
        self.signals_allowed
    }

    /// Returns the sandbox-authorized retained event ceiling.
    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }

    /// Returns the sandbox-authorized terminal output ceiling.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        match self.output_bytes {
            Some(value) => value,
            None => 0,
        }
    }

    /// Returns the optional sandbox-authorized cumulative output ceiling.
    #[must_use]
    pub const fn output_limit(self) -> Option<u64> {
        self.output_bytes
    }
}

/// Whether and how callers may write process input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StdinPolicy {
    /// Child input starts closed.
    Closed,
    /// Individual and cumulative writes are bounded.
    Bounded {
        /// Maximum bytes in one accepted write.
        max_write_bytes: u64,
        /// Maximum bytes accepted over the process lifetime.
        max_total_bytes: u64,
    },
    /// Individual writes are bounded while lifetime input remains open-ended.
    Streaming {
        /// Maximum bytes in one accepted write.
        max_write_bytes: u64,
    },
}

impl StdinPolicy {
    /// Creates a bounded stdin policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or inverted limits.
    pub const fn bounded(max_write_bytes: u64, max_total_bytes: u64) -> Result<Self, ProcessError> {
        if max_write_bytes == 0 || max_write_bytes > max_total_bytes {
            Err(invalid("stdin limits are zero or inverted"))
        } else {
            Ok(Self::Bounded { max_write_bytes, max_total_bytes })
        }
    }

    /// Creates a streaming stdin policy with a physical per-write allocation bound.
    ///
    /// # Errors
    /// Returns an error for a zero per-write bound.
    pub const fn streaming(max_write_bytes: u64) -> Result<Self, ProcessError> {
        if max_write_bytes == 0 {
            Err(invalid("stdin write bound is zero"))
        } else {
            Ok(Self::Streaming { max_write_bytes })
        }
    }
}

/// Action taken when a stream limit is reached.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputOverflowAction {
    /// Retain exact accounting, mark output incomplete, and continue the process.
    ContinueIncomplete,
    /// Record the output-limit trigger and terminate the owned tree.
    Terminate,
}

/// Independent bounded output/event policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OutputPolicy {
    chunk_bytes: u64,
    retained_window_bytes: u64,
    spool_segment_bytes: u64,
    spool_bytes: Option<u64>,
    event_count: u64,
    stdout_bytes: Option<u64>,
    stderr_bytes: Option<u64>,
    terminal_bytes: Option<u64>,
    overflow_action: OutputOverflowAction,
}

impl OutputPolicy {
    /// Creates complete output bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for zero/inconsistent bounds or in-memory values that exceed native
    /// addressability.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        chunk_bytes: u64,
        retained_window_bytes: u64,
        spool_bytes: u64,
        event_count: u64,
        stdout_bytes: u64,
        stderr_bytes: u64,
        terminal_bytes: u64,
        overflow_action: OutputOverflowAction,
    ) -> Result<Self, ProcessError> {
        let values_consistent = chunk_bytes > 0
            && retained_window_bytes > 0
            && spool_bytes > 0
            && event_count > 0
            && stdout_bytes > 0
            && stderr_bytes > 0
            && terminal_bytes > 0
            && chunk_bytes <= retained_window_bytes
            && chunk_bytes <= spool_bytes;
        if !values_consistent {
            return Err(invalid("output limits are zero or inconsistent"));
        }
        let maximum_addressable = usize::MAX as u64;
        if chunk_bytes > maximum_addressable
            || retained_window_bytes > maximum_addressable
            || event_count > maximum_addressable
        {
            return Err(invalid("in-memory output policy exceeds native addressability"));
        }
        Ok(Self {
            chunk_bytes,
            retained_window_bytes,
            spool_segment_bytes: spool_bytes,
            spool_bytes: Some(spool_bytes),
            event_count,
            stdout_bytes: Some(stdout_bytes),
            stderr_bytes: Some(stderr_bytes),
            terminal_bytes: Some(terminal_bytes),
            overflow_action,
        })
    }

    /// Creates streaming output with bounded physical buffers and no cumulative byte allowance.
    ///
    /// # Errors
    /// Returns an error for zero or inconsistent physical allocations.
    pub const fn streaming(
        chunk_bytes: u64,
        retained_window_bytes: u64,
        spool_segment_bytes: u64,
        retained_event_count: u64,
    ) -> Result<Self, ProcessError> {
        let values_consistent = chunk_bytes > 0
            && retained_window_bytes > 0
            && spool_segment_bytes > 0
            && retained_event_count > 0
            && chunk_bytes <= retained_window_bytes
            && chunk_bytes <= spool_segment_bytes;
        let maximum_addressable = usize::MAX as u64;
        if !values_consistent {
            return Err(invalid("streaming output allocations are zero or inconsistent"));
        }
        if chunk_bytes > maximum_addressable
            || retained_window_bytes > maximum_addressable
            || retained_event_count > maximum_addressable
        {
            return Err(invalid("in-memory output allocation exceeds native addressability"));
        }
        Ok(Self {
            chunk_bytes,
            retained_window_bytes,
            spool_segment_bytes,
            spool_bytes: None,
            event_count: retained_event_count,
            stdout_bytes: None,
            stderr_bytes: None,
            terminal_bytes: None,
            overflow_action: OutputOverflowAction::ContinueIncomplete,
        })
    }

    /// Returns the maximum read/event chunk.
    #[must_use]
    pub const fn chunk_bytes(self) -> u64 {
        self.chunk_bytes
    }
    /// Returns the in-memory retained window bound.
    #[must_use]
    pub const fn retained_window_bytes(self) -> u64 {
        self.retained_window_bytes
    }
    /// Returns the durable spool bound.
    #[must_use]
    pub const fn spool_bytes(self) -> u64 {
        match self.spool_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative durable spool allowance.
    #[must_use]
    pub const fn spool_limit(self) -> Option<u64> {
        self.spool_bytes
    }
    /// Returns the maximum physical size of one durable spool segment.
    #[must_use]
    pub const fn spool_segment_bytes(self) -> u64 {
        self.spool_segment_bytes
    }
    /// Returns the retained event count bound.
    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }
    /// Returns the total stdout observation bound.
    #[must_use]
    pub const fn stdout_bytes(self) -> u64 {
        match self.stdout_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative stdout allowance.
    #[must_use]
    pub const fn stdout_limit(self) -> Option<u64> {
        self.stdout_bytes
    }
    /// Returns the total stderr observation bound.
    #[must_use]
    pub const fn stderr_bytes(self) -> u64 {
        match self.stderr_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative stderr allowance.
    #[must_use]
    pub const fn stderr_limit(self) -> Option<u64> {
        self.stderr_bytes
    }
    /// Returns the total PTY observation bound.
    #[must_use]
    pub const fn terminal_bytes(self) -> u64 {
        match self.terminal_bytes {
            Some(value) => value,
            None => 0,
        }
    }
    /// Returns the optional cumulative PTY output allowance.
    #[must_use]
    pub const fn terminal_limit(self) -> Option<u64> {
        self.terminal_bytes
    }
    /// Returns the configured overflow action.
    #[must_use]
    pub const fn overflow_action(self) -> OutputOverflowAction {
        self.overflow_action
    }

    pub(crate) const fn uses_legacy_encoding(self) -> bool {
        self.spool_bytes.is_some()
            && self.stdout_bytes.is_some()
            && self.stderr_bytes.is_some()
            && self.terminal_bytes.is_some()
            && self.spool_segment_bytes == self.spool_bytes()
    }
}

/// Initial graceful-stop behavior.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GracefulAction {
    /// Close process input and allow normal shutdown.
    CloseInput,
    /// Send an interrupt to the owned Unix process group where supported.
    Interrupt,
    /// Request platform termination before forced kill.
    Terminate,
}

/// Cancellation strategy selected independently from wall-clock execution deadlines.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StopStrategy {
    /// Force-stop the exact owned process tree immediately.
    Force,
    /// Request a graceful action, then force-stop after the selected observation interval.
    GracefulThenForce {
        /// Initial graceful action.
        action: GracefulAction,
        /// Selected time before forced termination.
        escalation_millis: u64,
    },
}

/// Complete wall deadline and escalation bounds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeadlinePolicy {
    wall_timeout_millis: Option<u64>,
    stop_strategy: StopStrategy,
    reap_millis: Option<u64>,
}

impl DeadlinePolicy {
    /// Creates a checked deadline and escalation policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero escalation durations.
    pub const fn new(
        wall_timeout_millis: Option<u64>,
        graceful_action: GracefulAction,
        grace_millis: u64,
        reap_millis: u64,
    ) -> Result<Self, ProcessError> {
        Self::graceful(
            wall_timeout_millis,
            graceful_action,
            grace_millis,
            Some(reap_millis),
        )
    }

    /// Creates a graceful cancellation policy with optional cleanup observation patience.
    ///
    /// A missing reap interval retains ownership and observes cleanup without inventing a lifetime
    /// deadline. A selected interval bounds one cleanup observation attempt; expiration does not
    /// prove quiescence or authorize terminal publication.
    ///
    /// # Errors
    /// Returns an error for a selected zero wall timeout, escalation interval, or reap interval.
    pub const fn graceful(
        wall_timeout_millis: Option<u64>,
        graceful_action: GracefulAction,
        grace_millis: u64,
        reap_millis: Option<u64>,
    ) -> Result<Self, ProcessError> {
        let wall_valid = match wall_timeout_millis {
            Some(value) => value > 0,
            None => true,
        };
        if !wall_valid || grace_millis == 0 || matches!(reap_millis, Some(0)) {
            return Err(invalid("deadline or escalation duration is zero"));
        }
        Ok(Self {
            wall_timeout_millis,
            stop_strategy: StopStrategy::GracefulThenForce {
                action: graceful_action,
                escalation_millis: grace_millis,
            },
            reap_millis,
        })
    }

    /// Creates an immediate force-stop policy with no synthetic cleanup patience limit.
    ///
    /// # Errors
    /// Returns an error for a selected zero wall timeout.
    pub const fn force(wall_timeout_millis: Option<u64>) -> Result<Self, ProcessError> {
        if matches!(wall_timeout_millis, Some(0)) {
            Err(invalid("wall timeout is zero"))
        } else {
            Ok(Self { wall_timeout_millis, stop_strategy: StopStrategy::Force, reap_millis: None })
        }
    }

    /// Returns the optional wall timeout.
    #[must_use]
    pub const fn wall_timeout_millis(self) -> Option<u64> {
        self.wall_timeout_millis
    }
    /// Returns the initial graceful action.
    #[must_use]
    pub const fn stop_strategy(self) -> StopStrategy {
        self.stop_strategy
    }
    /// Returns time allowed after graceful action.
    #[must_use]
    pub const fn reap_millis(self) -> Option<u64> {
        self.reap_millis
    }

    pub(crate) const fn uses_legacy_encoding(self) -> bool {
        matches!(self.stop_strategy, StopStrategy::GracefulThenForce { .. })
            && self.reap_millis.is_some()
    }
}
