//! C5 model-protocol version identity.

use crate::{ProtocolError, ProtocolErrorKind};

/// Supported protocol major version.
pub const PROTOCOL_MAJOR: u16 = 1;
/// Current protocol minor version.
pub const PROTOCOL_MINOR: u16 = 0;

/// Checked model-protocol version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolVersion {
    major: u16,
    minor: u16,
}

impl ProtocolVersion {
    /// Version used by newly created C5 values.
    pub const V1: Self = Self { major: PROTOCOL_MAJOR, minor: PROTOCOL_MINOR };

    /// Accepts a version understood by this implementation.
    ///
    /// # Errors
    ///
    /// Rejects unknown majors and newer minors whose semantics are not implemented.
    pub fn new(major: u16, minor: u16) -> Result<Self, ProtocolError> {
        if major != PROTOCOL_MAJOR || minor > PROTOCOL_MINOR {
            return Err(ProtocolError::at(
                ProtocolErrorKind::UnsupportedVersion,
                "protocol_version",
                "model protocol version is unsupported",
            ));
        }
        Ok(Self { major, minor })
    }

    /// Returns the major component.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the minor component.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}
