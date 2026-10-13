//! Checked application-protocol versions and inclusive ranges.

use crate::{AppErrorCode, AppProtocolError};

/// Exact application-protocol version implemented by current clients and the daemon.
pub const CURRENT_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 2, minor: 1 };

/// Sole application-protocol range implemented by current clients and the daemon.
pub const CURRENT_PROTOCOL_RANGE: VersionRange =
    VersionRange { major: 2, minor_min: 1, minor_max: 1 };

/// One nonzero-major application-protocol version.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtocolVersion {
    major: u16,
    minor: u16,
}

impl ProtocolVersion {
    /// Creates a checked protocol version.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidVersion`] when `major` is zero.
    pub const fn new(major: u16, minor: u16) -> Result<Self, AppProtocolError> {
        if major == 0 {
            Err(AppProtocolError::new(AppErrorCode::InvalidVersion, None))
        } else {
            Ok(Self { major, minor })
        }
    }

    /// Returns the nonzero compatibility major.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }
    /// Returns the minor revision.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

/// Inclusive range of minor versions within one compatibility major.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VersionRange {
    major: u16,
    minor_min: u16,
    minor_max: u16,
}

impl VersionRange {
    /// Creates an inclusive range with one nonzero major.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidVersion`] for a zero major or reversed minor endpoints.
    pub const fn new(major: u16, minor_min: u16, minor_max: u16) -> Result<Self, AppProtocolError> {
        if major == 0 || minor_min > minor_max {
            Err(AppProtocolError::new(AppErrorCode::InvalidVersion, None))
        } else {
            Ok(Self { major, minor_min, minor_max })
        }
    }

    /// Returns the nonzero compatibility major.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }
    /// Returns the inclusive minimum minor.
    #[must_use]
    pub const fn minor_min(self) -> u16 {
        self.minor_min
    }
    /// Returns the inclusive maximum minor.
    #[must_use]
    pub const fn minor_max(self) -> u16 {
        self.minor_max
    }
    /// Returns the preferred version represented by this range.
    #[must_use]
    pub const fn preferred(self) -> ProtocolVersion {
        ProtocolVersion { major: self.major, minor: self.minor_max }
    }

    /// Returns whether this range contains a version.
    #[must_use]
    pub const fn contains(self, version: ProtocolVersion) -> bool {
        self.major == version.major
            && (self.minor_min <= version.minor && version.minor <= self.minor_max)
    }

    /// Returns the greatest version in this range's intersection with `other`.
    #[must_use]
    pub const fn greatest_intersection(self, other: Self) -> Option<ProtocolVersion> {
        if self.major != other.major {
            return None;
        }
        let minimum =
            if self.minor_min >= other.minor_min { self.minor_min } else { other.minor_min };
        let maximum =
            if self.minor_max <= other.minor_max { self.minor_max } else { other.minor_max };
        if minimum <= maximum {
            Some(ProtocolVersion { major: self.major, minor: maximum })
        } else {
            None
        }
    }
}

/// Descriptive alias for an application-protocol version range.
pub type ProtocolVersionRange = VersionRange;
