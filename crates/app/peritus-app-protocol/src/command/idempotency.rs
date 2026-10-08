//! Bounded active idempotency slots with immutable final-result history.

use std::collections::VecDeque;

use crate::{AppErrorCode, AppProtocolError, IdempotencyKey, RequestId};
use peritus_types::{ActorId, SessionId};

use super::{CommandBinding, CommandResult, RequestDigest};

/// One retained final durable-session/actor/key result binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyEntry {
    actor_id: ActorId,
    session_id: SessionId,
    key: IdempotencyKey,
    request_digest: RequestDigest,
    original_request_id: RequestId,
    result: CommandResult,
}

impl IdempotencyEntry {
    /// Returns the actor-scoped identity.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    /// Returns the durable session that scopes the actor/key pair.
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Borrows the exact idempotency key.
    #[must_use]
    pub const fn key(&self) -> &IdempotencyKey {
        &self.key
    }
    /// Returns the digest of the original completely bound request.
    #[must_use]
    pub const fn request_digest(&self) -> RequestDigest {
        self.request_digest
    }
    /// Returns the original application request identity.
    #[must_use]
    pub const fn original_request_id(&self) -> RequestId {
        self.original_request_id
    }
    /// Borrows the retained final result.
    #[must_use]
    pub const fn result(&self) -> &CommandResult {
        &self.result
    }
}

/// Pure admission classification before command execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdempotencyAdmission {
    /// No active slot or historical receipt uses this durable-session/actor/key, and one active
    /// slot was reserved for the exact request.
    New,
    /// The exact request already owns an active slot but does not yet have a final result.
    Pending {
        /// Identity of the originally admitted request.
        original_request_id: RequestId,
    },
    /// The exact request has a retained final result.
    Replay {
        /// Identity of the originally completed request.
        original_request_id: RequestId,
        /// Retained final result, with disposition rewritten to replay when successful.
        result: CommandResult,
    },
    /// The durable-session/actor/key exists but its complete request digest differs.
    Conflict {
        /// Identity of the request that already owns the key.
        original_request_id: RequestId,
    },
    /// No matching request exists and every active slot is occupied.
    Capacity,
}

/// Result of recording a final result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyRecordDisposition {
    /// A new final entry was appended.
    Stored,
    /// The same actor/key/request digest already had a retained final result.
    AlreadyRecorded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct IdempotencyReservation {
    actor_id: ActorId,
    session_id: SessionId,
    key: IdempotencyKey,
    request_digest: RequestDigest,
    original_request_id: RequestId,
}

/// Explicitly bounded active commands plus insertion-ordered immutable receipt history.
///
/// Capacity applies only to reservations and completed results that have not yet been
/// acknowledged through [`Self::retire_oldest`]. Retiring such a result releases its active slot
/// while moving the receipt into immutable history, where exact replay and conflict detection
/// remain available for the lifetime of this state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyWindow {
    capacity: usize,
    reservations: Vec<IdempotencyReservation>,
    unacknowledged: VecDeque<IdempotencyEntry>,
    history: VecDeque<IdempotencyEntry>,
}

impl IdempotencyWindow {
    /// Creates an empty bounded active-command window with immutable receipt history.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidLimits`] when `capacity` is zero.
    pub const fn new(capacity: usize) -> Result<Self, AppProtocolError> {
        if capacity == 0 {
            Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None))
        } else {
            Ok(Self {
                capacity,
                reservations: Vec::new(),
                unacknowledged: VecDeque::new(),
                history: VecDeque::new(),
            })
        }
    }

    /// Creates an empty window using the negotiated active-slot ceiling.
    ///
    /// # Errors
    ///
    /// Returns an invalid-limits error only if the supplied limit set violated its type invariant.
    pub const fn from_limits(limits: crate::AppProtocolLimits) -> Result<Self, AppProtocolError> {
        Self::new(limits.max_active_idempotency_slots())
    }

    /// Classifies a bound request and reserves one active slot when it is new.
    ///
    /// Exact active and historical identities remain reachable even while all other slots are
    /// occupied. A caller receiving [`IdempotencyAdmission::New`] must eventually record a final
    /// result; repeating the same request before then returns [`IdempotencyAdmission::Pending`].
    pub fn admit(&mut self, binding: &CommandBinding) -> IdempotencyAdmission {
        if let Some(entry) = self.find_receipt(binding) {
            return if entry.request_digest == binding.request_digest() {
                IdempotencyAdmission::Replay {
                    original_request_id: entry.original_request_id,
                    result: entry.result.as_replay(),
                }
            } else {
                IdempotencyAdmission::Conflict {
                    original_request_id: entry.original_request_id,
                }
            };
        }
        if let Some(reservation) = self.find_reservation(binding) {
            return if reservation.request_digest == binding.request_digest() {
                IdempotencyAdmission::Pending {
                    original_request_id: reservation.original_request_id,
                }
            } else {
                IdempotencyAdmission::Conflict {
                    original_request_id: reservation.original_request_id,
                }
            };
        }
        if self.active_len() >= self.capacity {
            return IdempotencyAdmission::Capacity;
        }
        self.reservations.push(IdempotencyReservation {
            actor_id: binding.actor_id(),
            session_id: binding.session_id(),
            key: binding.idempotency_key().clone(),
            request_digest: binding.request_digest(),
            original_request_id: binding.request_id(),
        });
        IdempotencyAdmission::New
    }

    /// Replaces the exact active reservation with an unacknowledged immutable final receipt.
    ///
    /// # Errors
    ///
    /// Returns a command-binding error when the final result names another original request or no
    /// exact active reservation exists, and an idempotency-conflict error for
    /// session/actor/key reuse with a different digest.
    pub fn record(
        &mut self,
        binding: &CommandBinding,
        result: CommandResult,
    ) -> Result<IdempotencyRecordDisposition, AppProtocolError> {
        if result.original_request_id() != binding.request_id() {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        if let Some(entry) = self.find_receipt(binding) {
            return if entry.request_digest == binding.request_digest() {
                Ok(IdempotencyRecordDisposition::AlreadyRecorded)
            } else {
                Err(AppProtocolError::new(AppErrorCode::IdempotencyConflict, None))
            };
        }
        let Some(index) = self.reservations.iter().position(|reservation| {
            reservation.actor_id == binding.actor_id()
                && reservation.session_id == binding.session_id()
                && reservation.key == *binding.idempotency_key()
        }) else {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        };
        if self.reservations[index].request_digest != binding.request_digest() {
            return Err(AppProtocolError::new(AppErrorCode::IdempotencyConflict, None));
        }
        let reservation = self.reservations.remove(index);
        self.unacknowledged.push_back(IdempotencyEntry {
            actor_id: binding.actor_id(),
            session_id: binding.session_id(),
            key: binding.idempotency_key().clone(),
            request_digest: binding.request_digest(),
            original_request_id: reservation.original_request_id,
            result,
        });
        Ok(IdempotencyRecordDisposition::Stored)
    }

    /// Acknowledges the oldest final result, releases its active slot, and archives its receipt.
    ///
    /// The returned value is a copy of the receipt now held in immutable history. Its
    /// durable-session/actor/key binding remains available to exact replay and conflict checks.
    pub fn retire_oldest(&mut self) -> Option<IdempotencyEntry> {
        let entry = self.unacknowledged.pop_front()?;
        self.history.push_back(entry.clone());
        Some(entry)
    }
    /// Returns the configured active-slot capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    /// Returns the number of occupied active slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.active_len()
    }
    /// Returns whether no command currently occupies an active slot.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.active_len() == 0
    }
    /// Iterates unacknowledged final receipts in oldest-to-newest acknowledgement order.
    #[must_use]
    pub fn entries(&self) -> std::collections::vec_deque::Iter<'_, IdempotencyEntry> {
        self.unacknowledged.iter()
    }
    /// Iterates every immutable receipt in completion order, including acknowledged history.
    #[must_use]
    pub fn receipts(&self) -> impl Iterator<Item = &IdempotencyEntry> {
        self.history.iter().chain(self.unacknowledged.iter())
    }
    /// Returns the number of acknowledged immutable historical receipts.
    #[must_use]
    pub fn historical_len(&self) -> usize {
        self.history.len()
    }

    fn active_len(&self) -> usize {
        self.reservations.len().saturating_add(self.unacknowledged.len())
    }

    fn find_receipt(&self, binding: &CommandBinding) -> Option<&IdempotencyEntry> {
        self.history.iter().chain(self.unacknowledged.iter()).find(|entry| {
            entry.actor_id == binding.actor_id()
                && entry.session_id == binding.session_id()
                && entry.key == *binding.idempotency_key()
        })
    }

    fn find_reservation(&self, binding: &CommandBinding) -> Option<&IdempotencyReservation> {
        self.reservations.iter().find(|reservation| {
            reservation.actor_id == binding.actor_id()
                && reservation.session_id == binding.session_id()
                && reservation.key == *binding.idempotency_key()
        })
    }
}
