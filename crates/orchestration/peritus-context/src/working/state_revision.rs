//! Checked reducer revision advancement.

use super::WorkingError;
use vstd::prelude::*;

verus! {
pub(super) const fn next_revision(current: u64) -> (next: Result<u64, WorkingError>)
    ensures match next {
        Ok(value) => value as int == current as int + 1 && value > current,
        Err(error) => current == u64::MAX && error == WorkingError::RevisionExhausted,
    },
{
    if current == u64::MAX { Err(WorkingError::RevisionExhausted) } else { Ok(current + 1) }
}
}
