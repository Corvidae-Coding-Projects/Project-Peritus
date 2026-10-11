//! Exact validation of canonical required and observed schema fields.

use super::{SchemaField, SchemaFieldId};
use crate::{ObligationError, ObligationErrorKind};
use core::cmp::Ordering;

fn required_pair_order(fields: &[SchemaField], index: usize) -> Ordering {
    let previous = fields[index - 1].id();
    let current = fields[index].id();
    crate::order::compare(previous.digest().as_bytes(), current.digest().as_bytes())
}
fn observed_pair_order(fields: &[SchemaFieldId], index: usize) -> Ordering {
    crate::order::compare(fields[index - 1].digest().as_bytes(), fields[index].digest().as_bytes())
}
pub(super) fn validate_required_fields(
    fields: &[SchemaField],
    maximum: usize,
) -> Result<(), ObligationError> {
    if fields.is_empty() || fields.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::InvalidSchema,
            maximum as u64,
            fields.len() as u64,
        ));
    }
    let mut index = 0;
    while index < fields.len() {
        if index > 0 {
            let order = required_pair_order(fields, index);
            match order {
                Ordering::Equal => {
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                Ordering::Greater => {
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn validate_observed_fields(
    fields: &[SchemaFieldId],
    maximum: usize,
) -> Result<(), ObligationError> {
    if fields.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::InvalidSchema,
            maximum as u64,
            fields.len() as u64,
        ));
    }
    let mut index = 0;
    while index < fields.len() {
        if index > 0 {
            let order = observed_pair_order(fields, index);
            match order {
                Ordering::Equal => {
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                Ordering::Greater => {
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
                Ordering::Less => {}
            }
        }
        index += 1;
    }
    Ok(())
}
