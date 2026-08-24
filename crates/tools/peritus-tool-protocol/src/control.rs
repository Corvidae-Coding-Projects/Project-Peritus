//! Supported-control declarations and bounded control envelopes.

use crate::{BoundedText, ProtocolError, ProtocolErrorKind};

/// Immutable set of controls supported by one tool implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlSet(u8);

impl ControlSet {
    const STDIN: u8 = 1;
    const RESIZE: u8 = 2;
    const SIGNAL: u8 = 4;
    const CANCEL: u8 = 8;
    const POLL: u8 = 16;

    /// No running controls; the tool must return a terminal result.
    pub const NONE: Self = Self(0);

    /// Creates a supported-control set.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "each bit is a distinct protocol control capability"
    )]
    pub const fn new(stdin: bool, resize: bool, signal: bool, cancel: bool, poll: bool) -> Self {
        Self(
            (stdin as u8)
                | ((resize as u8) << 1)
                | ((signal as u8) << 2)
                | ((cancel as u8) << 3)
                | ((poll as u8) << 4),
        )
    }

    /// Returns whether bounded stdin is supported.
    #[must_use]
    pub const fn stdin(self) -> bool {
        self.0 & Self::STDIN != 0
    }
    /// Returns whether PTY resize is supported.
    #[must_use]
    pub const fn resize(self) -> bool {
        self.0 & Self::RESIZE != 0
    }
    /// Returns whether named signals are supported.
    #[must_use]
    pub const fn signal(self) -> bool {
        self.0 & Self::SIGNAL != 0
    }
    /// Returns whether cancellation is supported.
    #[must_use]
    pub const fn cancel(self) -> bool {
        self.0 & Self::CANCEL != 0
    }
    /// Returns whether polling is supported.
    #[must_use]
    pub const fn poll(self) -> bool {
        self.0 & Self::POLL != 0
    }
    pub(crate) const fn bits(self) -> u8 {
        self.0
    }
}

/// Stable reason for router-mediated cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationReason {
    /// A caller explicitly cancelled the invocation.
    Requested,
    /// The immutable call deadline elapsed.
    Deadline,
    /// The owning session is shutting down.
    Shutdown,
    /// Recovery established that continuation is unsafe.
    Recovery,
}

/// One bounded control request for an active invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolControl {
    /// Poll without injecting another control.
    Poll,
    /// Write bounded input bytes.
    Stdin(Vec<u8>),
    /// Resize a PTY to nonzero rows and columns.
    Resize {
        /// Nonzero terminal row count.
        rows: u16,
        /// Nonzero terminal column count.
        columns: u16,
    },
    /// Deliver a validated stable signal name.
    Signal(BoundedText),
    /// Request cancellation.
    Cancel(CancellationReason),
}

impl ToolControl {
    /// Creates bounded stdin input.
    ///
    /// # Errors
    ///
    /// Rejects empty input or more than the descriptor's per-control byte ceiling.
    pub fn stdin(bytes: Vec<u8>, maximum: u32) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > maximum as usize {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "control.stdin",
                "stdin control is empty or exceeds its byte bound",
            ));
        }
        Ok(Self::Stdin(bytes))
    }

    /// Creates a nonzero terminal resize.
    ///
    /// # Errors
    ///
    /// Rejects a zero row or column count.
    pub fn resize(rows: u16, columns: u16) -> Result<Self, ProtocolError> {
        if rows == 0 || columns == 0 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "control.resize",
                "terminal dimensions must be nonzero",
            ));
        }
        Ok(Self::Resize { rows, columns })
    }

    /// Returns stable version-one canonical control-envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(7);
        match self {
            Self::Poll => bytes.push(1),
            Self::Stdin(value) => {
                bytes.push(2);
                crate::wire::bytes(&mut bytes, value);
            }
            Self::Resize { rows, columns } => {
                bytes.push(3);
                crate::wire::u16_value(&mut bytes, *rows);
                crate::wire::u16_value(&mut bytes, *columns);
            }
            Self::Signal(value) => {
                bytes.push(4);
                crate::wire::text(&mut bytes, value.as_str());
            }
            Self::Cancel(reason) => {
                bytes.push(5);
                bytes.push(match reason {
                    CancellationReason::Requested => 1,
                    CancellationReason::Deadline => 2,
                    CancellationReason::Shutdown => 3,
                    CancellationReason::Recovery => 4,
                });
            }
        }
        bytes
    }
}
