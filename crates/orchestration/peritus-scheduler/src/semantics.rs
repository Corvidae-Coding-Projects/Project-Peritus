//! Closed scheduler queue and recovery semantics.

#[doc = " Immutable queue and recovery semantics for one scheduler aggregate."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchedulerSemantics {
    #[doc = " Historical queue admission and recovery behavior encoded as schema 1."]
    LegacyQueueV1,
    #[doc = " Recovery-aware queue pressure encoded as schema 2."]
    StrictRecoveryQueueV2,
}
impl SchedulerSemantics {
    #[doc = " Returns the exact scheduler wire schema carrying these semantics."]
    #[must_use]
    pub const fn schema_version(self) -> u16 {
        match self {
            Self::LegacyQueueV1 => 1,
            Self::StrictRecoveryQueueV2 => 2,
        }
    }
    #[doc = " Maps a supported scheduler wire schema to its aggregate semantics."]
    #[must_use]
    pub const fn from_schema_version(schema_version: u16) -> Option<Self> {
        match schema_version {
            1 => Some(Self::LegacyQueueV1),
            2 => Some(Self::StrictRecoveryQueueV2),
            _ => None,
        }
    }
    #[doc = " Compares exact aggregate semantics for verified production fences."]
    pub(crate) const fn same(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::LegacyQueueV1, Self::LegacyQueueV1)
                | (Self::StrictRecoveryQueueV2, Self::StrictRecoveryQueueV2)
        )
    }
}
