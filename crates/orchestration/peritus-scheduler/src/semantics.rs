//! Closed scheduler queue and recovery semantics.

use vstd::prelude::*;

verus! {

/// Immutable queue and recovery semantics for one scheduler aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerSemantics {
    /// Historical queue admission and recovery behavior encoded as schema 1.
    LegacyQueueV1,
    /// Recovery-aware queue pressure encoded as schema 2.
    StrictRecoveryQueueV2,
}

impl SchedulerSemantics {
    /// Mathematical schema identity for these scheduler semantics.
    pub open spec fn spec_schema_version(self) -> u16 {
        match self {
            Self::LegacyQueueV1 => 1,
            Self::StrictRecoveryQueueV2 => 2,
        }
    }

    /// Exact partial inverse of the supported scheduler schema mapping.
    pub open spec fn spec_from_schema_version(schema_version: u16) -> Option<Self> {
        match schema_version {
            1 => Some(Self::LegacyQueueV1),
            2 => Some(Self::StrictRecoveryQueueV2),
            _ => None,
        }
    }

    /// Returns the exact scheduler wire schema carrying these semantics.
    #[must_use]
    pub const fn schema_version(self) -> (schema_version: u16)
        ensures schema_version == self.spec_schema_version(),
    {
        match self {
            Self::LegacyQueueV1 => 1,
            Self::StrictRecoveryQueueV2 => 2,
        }
    }

    /// Maps a supported scheduler wire schema to its aggregate semantics.
    #[must_use]
    pub const fn from_schema_version(schema_version: u16) -> (semantics: Option<Self>)
        ensures semantics == Self::spec_from_schema_version(schema_version),
    {
        match schema_version {
            1 => Some(Self::LegacyQueueV1),
            2 => Some(Self::StrictRecoveryQueueV2),
            _ => None,
        }
    }

    /// Compares exact aggregate semantics for verified production fences.
    pub(crate) const fn same(self, other: Self) -> (same: bool)
        ensures same == (self == other),
    {
        matches!(
            (self, other),
            (Self::LegacyQueueV1, Self::LegacyQueueV1)
                | (Self::StrictRecoveryQueueV2, Self::StrictRecoveryQueueV2)
        )
    }
}

} // verus!
