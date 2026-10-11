//! Canonical path validation with exact failure precedence.

use super::PathMention;
use crate::{ObligationError, ObligationErrorKind};
use core::cmp::Ordering;

pub(super) fn validate(paths: &[PathMention], maximum: usize) -> Result<(), ObligationError> {
    if paths.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::LimitExceeded,
            maximum as u64,
            paths.len() as u64,
        ));
    }
    let mut index = 0;
    while index < paths.len() {
        if index > 0 {
            let previous = paths[index - 1].id();
            let current = paths[index].id();
            let order =
                crate::order::compare(previous.digest().as_bytes(), current.digest().as_bytes());
            match order {
                Ordering::Equal => {
                    let error = ObligationError::plain(ObligationErrorKind::DuplicateValue);
                    return Err(error);
                }
                Ordering::Greater => {
                    let error = ObligationError::plain(ObligationErrorKind::NonCanonicalOrder);
                    return Err(error);
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
