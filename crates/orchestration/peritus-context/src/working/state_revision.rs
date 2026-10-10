//! Checked reducer revision advancement.

use super::WorkingError;

pub(super) const fn next_revision(current: u64) -> Result<u64, WorkingError> {
    if current == u64::MAX { Err(WorkingError::RevisionExhausted) } else { Ok(current + 1) }
}
