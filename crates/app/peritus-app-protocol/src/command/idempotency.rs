//! Bounded pure final-result idempotency window.

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
    /// No retained entry uses this durable-session/actor/key and capacity remains.
    New,
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
    /// No matching entry exists and the bounded window is full.
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

/// Insertion-ordered, explicitly bounded final-result idempotency state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyWindow {
    capacity: usize,
    entries: VecDeque<IdempotencyEntry>,
}

impl IdempotencyWindow {
    /// Creates an empty bounded final-result window.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidLimits`] when `capacity` is zero.
    pub const fn new(capacity: usize) -> Result<Self, AppProtocolError> {
        if capacity == 0 {
            Err(AppProtocolError::new(AppErrorCode::InvalidLimits, None))
        } else {
            Ok(Self { capacity, entries: VecDeque::new() })
        }
    }

    /// Creates an empty window using the negotiated retained-entry ceiling.
    ///
    /// # Errors
    ///
    /// Returns an invalid-limits error only if the supplied limit set violated its type invariant.
    pub const fn from_limits(limits: crate::AppProtocolLimits) -> Result<Self, AppProtocolError> {
        Self::new(limits.max_idempotency_entries())
    }

    /// Classifies a bound request without mutating retained state.
    #[must_use]
    pub fn admit(&self, binding: &CommandBinding) -> IdempotencyAdmission {
        self.find(binding).map_or(
            if self.entries.len() >= self.capacity {
                IdempotencyAdmission::Capacity
            } else {
                IdempotencyAdmission::New
            },
            |entry| {
                if entry.request_digest == binding.request_digest() {
                    IdempotencyAdmission::Replay {
                        original_request_id: entry.original_request_id,
                        result: entry.result.as_replay(),
                    }
                } else {
                    IdempotencyAdmission::Conflict {
                        original_request_id: entry.original_request_id,
                    }
                }
            },
        )
    }

    /// Records a final result after a `New` admission.
    ///
    /// # Errors
    ///
    /// Returns a command-binding error when the final result names another original request, an
    /// idempotency-conflict error for session/actor/key reuse with a different digest, or an
    /// idempotency-capacity error when no entry can be appended. Callers must explicitly retire an
    /// entry before retrying a capacity failure.
    pub fn record(
        &mut self,
        binding: &CommandBinding,
        result: CommandResult,
    ) -> Result<IdempotencyRecordDisposition, AppProtocolError> {
        if result.original_request_id() != binding.request_id() {
            return Err(AppProtocolError::new(AppErrorCode::CommandBindingMismatch, None));
        }
        if let Some(entry) = self.find(binding) {
            return if entry.request_digest == binding.request_digest() {
                Ok(IdempotencyRecordDisposition::AlreadyRecorded)
            } else {
                Err(AppProtocolError::new(AppErrorCode::IdempotencyConflict, None))
            };
        }
        if self.entries.len() >= self.capacity {
            return Err(AppProtocolError::new(AppErrorCode::IdempotencyCapacity, None));
        }
        self.entries.push_back(IdempotencyEntry {
            actor_id: binding.actor_id(),
            session_id: binding.session_id(),
            key: binding.idempotency_key().clone(),
            request_digest: binding.request_digest(),
            original_request_id: binding.request_id(),
            result,
        });
        Ok(IdempotencyRecordDisposition::Stored)
    }

    /// Explicitly retires and returns the oldest retained final entry.
    pub fn retire_oldest(&mut self) -> Option<IdempotencyEntry> {
        self.entries.pop_front()
    }
    /// Returns the configured entry capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    /// Returns the number of retained final entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Returns whether no final entries are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Iterates retained entries in explicit oldest-to-newest retirement order.
    #[must_use]
    pub fn entries(&self) -> std::collections::vec_deque::Iter<'_, IdempotencyEntry> {
        self.entries.iter()
    }

    fn find(&self, binding: &CommandBinding) -> Option<&IdempotencyEntry> {
        self.entries.iter().find(|entry| {
            entry.actor_id == binding.actor_id()
                && entry.session_id == binding.session_id()
                && entry.key == *binding.idempotency_key()
        })
    }
}
