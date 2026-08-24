//! Checked pipe, PTY, input, output, and deadline policies.

use crate::{ProcessError, error::invalid};

const MAX_OUTPUT_BOUND: u64 = 16 * 1_024 * 1_024 * 1_024;
const MAX_EVENT_COUNT: u64 = 16 * 1_024 * 1_024;
const MAX_INPUT_WRITE_BYTES: u64 = 16 * 1_024 * 1_024;
const MAX_DURATION_MILLIS: u64 = 30 * 24 * 60 * 60 * 1_000;

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
    output_bytes: u64,
}

impl TerminalCapabilities {
    pub(crate) const fn new(
        resize_allowed: bool,
        signals_allowed: bool,
        event_count: u64,
        output_bytes: u64,
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
}

impl StdinPolicy {
    /// Creates a bounded stdin policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero, inverted, or unreasonable limits.
    pub const fn bounded(max_write_bytes: u64, max_total_bytes: u64) -> Result<Self, ProcessError> {
        if max_write_bytes == 0
            || max_write_bytes > max_total_bytes
            || max_total_bytes > MAX_INPUT_WRITE_BYTES
        {
            Err(invalid("stdin limits are zero, inverted, or exceed the production bound"))
        } else {
            Ok(Self::Bounded { max_write_bytes, max_total_bytes })
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
    spool_bytes: u64,
    event_count: u64,
    stdout_bytes: u64,
    stderr_bytes: u64,
    terminal_bytes: u64,
    overflow_action: OutputOverflowAction,
}

impl OutputPolicy {
    /// Creates complete output bounds.
    ///
    /// # Errors
    ///
    /// Returns an error for zero operational bounds, an oversized value, or a chunk larger than
    /// the retained window or spool.
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
        let values_in_bound = chunk_bytes > 0
            && retained_window_bytes > 0
            && spool_bytes > 0
            && event_count > 0
            && chunk_bytes <= retained_window_bytes
            && chunk_bytes <= spool_bytes
            && retained_window_bytes <= MAX_OUTPUT_BOUND
            && spool_bytes <= MAX_OUTPUT_BOUND
            && stdout_bytes <= MAX_OUTPUT_BOUND
            && stderr_bytes <= MAX_OUTPUT_BOUND
            && terminal_bytes <= MAX_OUTPUT_BOUND
            && event_count <= MAX_EVENT_COUNT;
        if !values_in_bound {
            return Err(invalid(
                "output limits are zero, inconsistent, or exceed production bounds",
            ));
        }
        Ok(Self {
            chunk_bytes,
            retained_window_bytes,
            spool_bytes,
            event_count,
            stdout_bytes,
            stderr_bytes,
            terminal_bytes,
            overflow_action,
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
        self.spool_bytes
    }
    /// Returns the retained event count bound.
    #[must_use]
    pub const fn event_count(self) -> u64 {
        self.event_count
    }
    /// Returns the total stdout observation bound.
    #[must_use]
    pub const fn stdout_bytes(self) -> u64 {
        self.stdout_bytes
    }
    /// Returns the total stderr observation bound.
    #[must_use]
    pub const fn stderr_bytes(self) -> u64 {
        self.stderr_bytes
    }
    /// Returns the total PTY observation bound.
    #[must_use]
    pub const fn terminal_bytes(self) -> u64 {
        self.terminal_bytes
    }
    /// Returns the configured overflow action.
    #[must_use]
    pub const fn overflow_action(self) -> OutputOverflowAction {
        self.overflow_action
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

/// Complete wall deadline and escalation bounds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeadlinePolicy {
    wall_timeout_millis: Option<u64>,
    graceful_action: GracefulAction,
    grace_millis: u64,
    reap_millis: u64,
}

impl DeadlinePolicy {
    /// Creates a checked deadline and escalation policy.
    ///
    /// # Errors
    ///
    /// Returns an error for zero or over-limit durations.
    pub const fn new(
        wall_timeout_millis: Option<u64>,
        graceful_action: GracefulAction,
        grace_millis: u64,
        reap_millis: u64,
    ) -> Result<Self, ProcessError> {
        let wall_valid = match wall_timeout_millis {
            Some(value) => value > 0 && value <= MAX_DURATION_MILLIS,
            None => true,
        };
        if !wall_valid
            || grace_millis == 0
            || reap_millis == 0
            || grace_millis > MAX_DURATION_MILLIS
            || reap_millis > MAX_DURATION_MILLIS
        {
            return Err(invalid("deadline or escalation duration is zero or exceeds its bound"));
        }
        Ok(Self { wall_timeout_millis, graceful_action, grace_millis, reap_millis })
    }

    /// Returns the optional wall timeout.
    #[must_use]
    pub const fn wall_timeout_millis(self) -> Option<u64> {
        self.wall_timeout_millis
    }
    /// Returns the initial graceful action.
    #[must_use]
    pub const fn graceful_action(self) -> GracefulAction {
        self.graceful_action
    }
    /// Returns time allowed after graceful action.
    #[must_use]
    pub const fn grace_millis(self) -> u64 {
        self.grace_millis
    }
    /// Returns time allowed to reap after forced termination.
    #[must_use]
    pub const fn reap_millis(self) -> u64 {
        self.reap_millis
    }
}
