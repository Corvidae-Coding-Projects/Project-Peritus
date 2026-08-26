//! Reproducible paired seed derivation and C5 mapping.

use crate::{ProfileDigest, TaskId};

const SEED_DOMAIN: &[u8] = b"peritus.evaluation.rollout-seed.v1\0";

/// Exact paired seed shared by baseline and candidate for one task/ordinal.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RolloutSeed([u8; 32]);

impl RolloutSeed {
    pub(crate) fn derive(profile: ProfileDigest, task: TaskId, ordinal: u16) -> Self {
        let mut bytes = Vec::with_capacity(SEED_DOMAIN.len() + 32 + 16 + 2);
        bytes.extend_from_slice(SEED_DOMAIN);
        bytes.extend_from_slice(profile.as_bytes());
        bytes.extend_from_slice(task.as_bytes());
        bytes.extend_from_slice(&ordinal.to_be_bytes());
        Self(*peritus_codec::sha256(&bytes).as_bytes())
    }

    /// Borrows exact seed bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Maps the frozen seed to C5's signed 64-bit sampling field.
    #[must_use]
    pub const fn provider_seed(self) -> i64 {
        i64::from_be_bytes([
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7],
        ])
    }
}
