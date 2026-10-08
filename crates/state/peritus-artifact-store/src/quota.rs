//! Checked quota accounting plans.

use crate::{ArtifactStoreError, ErrorCode, RecoveryClass, verified::checked_quota_totals};

/// Observed logical quota usage before a reservation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_field_names)]
pub struct QuotaSnapshot {
    committed_bytes: u64,
    reserved_bytes: u64,
    limit_bytes: Option<u64>,
}

impl QuotaSnapshot {
    /// Validates one quota observation.
    ///
    /// # Errors
    ///
    /// Returns overflow when accounting cannot be represented, or quota exhaustion when existing
    /// accounting is already beyond the configured limit.
    pub const fn new(
        committed_bytes: u64,
        reserved_bytes: u64,
        limit_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        if limit_bytes == 0 {
            return Err(ArtifactStoreError::message(
                ErrorCode::InvalidConfiguration,
                RecoveryClass::CorrectRequest,
                "quota limit must be positive",
            ));
        }
        match checked_quota_totals(committed_bytes, reserved_bytes, 0) {
            None => Err(overflow()),
            Some((_, total)) if total > limit_bytes => Err(exceeded(total, limit_bytes)),
            Some(_) => Ok(Self {
                committed_bytes,
                reserved_bytes,
                limit_bytes: Some(limit_bytes),
            }),
        }
    }

    pub(crate) const fn for_policy(
        committed_bytes: u64,
        reserved_bytes: u64,
        limit_bytes: Option<u64>,
    ) -> Result<Self, ArtifactStoreError> {
        match limit_bytes {
            Some(limit) => Self::new(committed_bytes, reserved_bytes, limit),
            None => match checked_quota_totals(committed_bytes, reserved_bytes, 0) {
                Some(_) => Ok(Self { committed_bytes, reserved_bytes, limit_bytes: None }),
                None => Err(overflow()),
            },
        }
    }

    /// Returns finalized, committed logical bytes.
    #[must_use]
    pub const fn committed_bytes(self) -> u64 {
        self.committed_bytes
    }

    /// Returns finalized logical bytes under the original compatibility name.
    #[must_use]
    pub const fn used_bytes(self) -> u64 {
        self.committed_bytes
    }

    /// Returns bytes held by in-progress reservations.
    #[must_use]
    pub const fn reserved_bytes(self) -> u64 {
        self.reserved_bytes
    }

    /// Returns the configured logical byte limit, or `None` for physical-capacity policy.
    #[must_use]
    pub const fn limit_bytes(self) -> Option<u64> {
        self.limit_bytes
    }
}

/// Deterministic checked reservation plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaPlan {
    before: QuotaSnapshot,
    reservation_bytes: u64,
    reserved_after: u64,
    total_after: u64,
}

impl QuotaPlan {
    /// Plans a reservation without changing durable accounting.
    ///
    /// # Errors
    ///
    /// Returns a typed overflow or quota-exhaustion error.
    pub const fn reserve(
        before: QuotaSnapshot,
        reservation_bytes: u64,
    ) -> Result<Self, ArtifactStoreError> {
        let Some((reserved_after, total_after)) =
            checked_quota_totals(
                before.committed_bytes,
                before.reserved_bytes,
                reservation_bytes,
            )
        else {
            return Err(overflow());
        };
        if let Some(limit) = before.limit_bytes
            && total_after > limit
        {
            return Err(exceeded(total_after, limit));
        }
        Ok(Self { before, reservation_bytes, reserved_after, total_after })
    }

    /// Returns the exact observation this plan was based on.
    #[must_use]
    pub const fn before(self) -> QuotaSnapshot {
        self.before
    }

    /// Returns the new reservation size.
    #[must_use]
    pub const fn reservation_bytes(self) -> u64 {
        self.reservation_bytes
    }

    /// Returns all reserved bytes after applying the plan.
    #[must_use]
    pub const fn reserved_after(self) -> u64 {
        self.reserved_after
    }

    /// Returns used plus reserved bytes after applying the plan.
    #[must_use]
    pub const fn total_after(self) -> u64 {
        self.total_after
    }
}

const fn overflow() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::CorrectRequest,
        "quota byte accounting overflowed",
    )
}

const fn exceeded(attempted: u64, limit: u64) -> ArtifactStoreError {
    ArtifactStoreError::limit(ErrorCode::QuotaExceeded, attempted, limit)
}
