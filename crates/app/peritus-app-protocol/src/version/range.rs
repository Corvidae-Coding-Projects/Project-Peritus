//! Checked application-protocol versions and inclusive ranges.

use crate::{AppErrorCode, AppProtocolError};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_rejects_zero_major_and_reversal() {
        assert!(VersionRange::new(0, 0, 1).is_err());
        assert!(VersionRange::new(1, 2, 1).is_err());
    }

    #[test]
    fn intersection_selects_greatest_common_minor() {
        let left = VersionRange::new(2, 1, 7).unwrap();
        let right = VersionRange::new(2, 4, 9).unwrap();
        assert_eq!(left.greatest_intersection(right), ProtocolVersion::new(2, 7).ok());
    }
}
