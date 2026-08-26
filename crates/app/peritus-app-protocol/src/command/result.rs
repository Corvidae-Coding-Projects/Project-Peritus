//! Final protocol-level command disposition and committed event ranges.

use crate::{AppErrorCode, AppProtocolError, EventCursor, RequestId};
use vstd::prelude::*;

verus! {

/// Mathematical validity predicate for a positive inclusive committed cursor range.
pub open spec fn valid_committed_event_range(first: int, last: int) -> bool {
    0 < first && first <= last
}

/// An inclusive valid range has a positive exact count.
pub proof fn committed_range_count_positive(first: int, last: int)
    requires valid_committed_event_range(first, last)
    ensures last - first + 1 > 0
{
}

} // verus!

/// Positive, contiguous inclusive cursor range committed by one command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CommittedEventRange {
    first: EventCursor,
    last: EventCursor,
    count: u64,
}

impl CommittedEventRange {
    /// Creates a positive non-reversed inclusive range with an exactly representable count.
    ///
    /// # Errors
    ///
    /// Returns [`AppErrorCode::InvalidEventRange`] for origin, reversal, or arithmetic overflow.
    pub fn new(first: EventCursor, last: EventCursor) -> Result<Self, AppProtocolError> {
        if first.get() == 0 || first > last {
            return Err(AppProtocolError::new(AppErrorCode::InvalidEventRange, None));
        }
        let count = last
            .get()
            .checked_sub(first.get())
            .and_then(|difference| difference.checked_add(1))
            .ok_or_else(|| AppProtocolError::new(AppErrorCode::InvalidEventRange, None))?;
        Ok(Self { first, last, count })
    }

    /// Returns the first committed cursor.
    #[must_use]
    pub const fn first(self) -> EventCursor {
        self.first
    }
    /// Returns the last committed cursor.
    #[must_use]
    pub const fn last(self) -> EventCursor {
        self.last
    }
    /// Returns the exact inclusive cursor count `last - first + 1`.
    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// Stable protocol disposition of a completed command submission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CommandDisposition {
    /// This submission caused a new command commit.
    Committed,
    /// This submission replayed a retained final idempotent result.
    Replayed,
    /// This submission was rejected without a new commit.
    Rejected,
}

impl CommandDisposition {
    /// Returns the permanently assigned version-one wire tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Committed => 1,
            Self::Replayed => 2,
            Self::Rejected => 3,
        }
    }

    /// Recovers a command disposition from its permanently assigned wire tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::Committed),
            2 => Some(Self::Replayed),
            3 => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// Final application command result; this is not a durable C0 receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandResult {
    original_request_id: RequestId,
    disposition: CommandDisposition,
    committed_events: Option<CommittedEventRange>,
    error: Option<AppProtocolError>,
}

impl CommandResult {
    /// Creates a newly committed result with an exact nonempty event range.
    #[must_use]
    pub const fn committed(
        original_request_id: RequestId,
        committed_events: CommittedEventRange,
    ) -> Self {
        Self {
            original_request_id,
            disposition: CommandDisposition::Committed,
            committed_events: Some(committed_events),
            error: None,
        }
    }

    /// Creates a replay result preserving the original exact nonempty event range.
    #[must_use]
    pub const fn replayed(
        original_request_id: RequestId,
        committed_events: CommittedEventRange,
    ) -> Self {
        Self {
            original_request_id,
            disposition: CommandDisposition::Replayed,
            committed_events: Some(committed_events),
            error: None,
        }
    }

    /// Creates a rejected result with one machine-actionable error.
    #[must_use]
    pub const fn rejected(original_request_id: RequestId, error: AppProtocolError) -> Self {
        Self {
            original_request_id,
            disposition: CommandDisposition::Rejected,
            committed_events: None,
            error: Some(error),
        }
    }

    /// Clones a retained final result for an idempotent replay response.
    #[must_use]
    pub fn as_replay(&self) -> Self {
        match self.disposition {
            CommandDisposition::Committed | CommandDisposition::Replayed => Self {
                original_request_id: self.original_request_id,
                disposition: CommandDisposition::Replayed,
                committed_events: self.committed_events,
                error: None,
            },
            CommandDisposition::Rejected => self.clone(),
        }
    }

    /// Returns the identity of the original completed request.
    #[must_use]
    pub const fn original_request_id(&self) -> RequestId {
        self.original_request_id
    }
    /// Returns the stable final disposition.
    #[must_use]
    pub const fn disposition(&self) -> CommandDisposition {
        self.disposition
    }
    /// Returns the optional exact committed event range.
    #[must_use]
    pub const fn committed_events(&self) -> Option<CommittedEventRange> {
        self.committed_events
    }
    /// Borrows the rejection error, when disposition is rejected.
    #[must_use]
    pub const fn error(&self) -> Option<&AppProtocolError> {
        self.error.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_range_is_positive_and_counts_inclusively() {
        assert!(CommittedEventRange::new(EventCursor::origin(), EventCursor::new(1)).is_err());
        let range = CommittedEventRange::new(EventCursor::new(7), EventCursor::new(9)).unwrap();
        assert_eq!(range.count(), 3);
        let request_id = RequestId::new([1; 16]).unwrap();
        assert_eq!(CommandResult::committed(request_id, range).committed_events(), Some(range));
    }
}
