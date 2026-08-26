//! Closed evaluation-corpus partitions.

/// Visibility and operational purpose of one task partition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DatasetPartition {
    /// Visible development tasks used while authoring changes.
    Development,
    /// Visible calibration tasks used to freeze policies and thresholds.
    Calibration,
    /// Retained regression tasks.
    Regression,
    /// Sealed holdout tasks whose evaluator material is isolated.
    SealedHoldout,
    /// Canary tasks used for bounded pre-production observation.
    Canary,
}

impl DatasetPartition {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Development => 1,
            Self::Calibration => 2,
            Self::Regression => 3,
            Self::SealedHoldout => 4,
            Self::Canary => 5,
        }
    }
}
