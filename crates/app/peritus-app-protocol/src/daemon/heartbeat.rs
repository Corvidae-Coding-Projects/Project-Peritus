//! Monotonic heartbeat nonce observations without wall-clock semantics.

use crate::HeartbeatId;

use super::{DaemonControlError, DaemonControlErrorKind, DaemonStatus, error::reject};

/// One heartbeat nonce and monotonic sequence observation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DaemonHeartbeat {
    heartbeat_id: HeartbeatId,
    sequence: u64,
    status: DaemonStatus,
}

impl DaemonHeartbeat {
    /// Creates a heartbeat without adding wall-clock or liveness claims.
    #[must_use]
    pub const fn new(heartbeat_id: HeartbeatId, sequence: u64, status: DaemonStatus) -> Self {
        Self { heartbeat_id, sequence, status }
    }
    /// Returns the distinct heartbeat nonce.
    #[must_use]
    pub const fn heartbeat_id(&self) -> HeartbeatId {
        self.heartbeat_id
    }
    /// Returns the monotonic sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Borrows the observed status.
    #[must_use]
    pub const fn status(&self) -> &DaemonStatus {
        &self.status
    }
}

/// Pure heartbeat ordering state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartbeatState {
    next_sequence: u64,
    last: Option<DaemonHeartbeat>,
}

impl HeartbeatState {
    /// Creates an ordering state with an explicit first expected sequence.
    #[must_use]
    pub const fn new(first_sequence: u64) -> Self {
        Self { next_sequence: first_sequence, last: None }
    }
    /// Returns the exact next expected sequence.
    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }
    /// Borrows the latest accepted heartbeat.
    #[must_use]
    pub const fn last(&self) -> Option<&DaemonHeartbeat> {
        self.last.as_ref()
    }

    /// Accepts a distinct nonce at the exact next sequence.
    ///
    /// # Errors
    ///
    /// Rejects a repeated nonce, noncontiguous sequence, or sequence overflow.
    pub fn observe(&mut self, heartbeat: DaemonHeartbeat) -> Result<(), DaemonControlError> {
        if heartbeat.sequence != self.next_sequence {
            return Err(reject(
                DaemonControlErrorKind::HeartbeatOrdering,
                "heartbeat sequence is not the exact expected sequence",
            ));
        }
        if self.last.as_ref().is_some_and(|last| last.heartbeat_id == heartbeat.heartbeat_id) {
            return Err(reject(
                DaemonControlErrorKind::HeartbeatOrdering,
                "heartbeat nonce repeats the retained nonce",
            ));
        }
        let next = self.next_sequence.checked_add(1).ok_or_else(|| {
            reject(DaemonControlErrorKind::HeartbeatOrdering, "heartbeat sequence overflow")
        })?;
        self.last = Some(heartbeat);
        self.next_sequence = next;
        Ok(())
    }
}
