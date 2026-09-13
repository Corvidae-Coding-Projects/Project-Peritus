//! Exact validation of canonical required and observed schema fields.

use super::{SchemaField, SchemaFieldId};
use crate::{ObligationError, ObligationErrorKind};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

proof fn required_nonless_is_not_ordered(
    fields: Seq<SchemaField>,
    index: int,
    order: Ordering,
)
    requires
        1 <= index < fields.len(),
        order == crate::order::byte_order(
            fields[index - 1].spec_id().spec_digest().spec_bytes()@,
            fields[index].spec_id().spec_digest().spec_bytes()@,
        ),
        order != Ordering::Less,
    ensures !super::model::required_fields_ordered(fields),
{
    if super::model::required_fields_ordered(fields) {
        super::model::required_ordered_at(fields, index);
    }
}

proof fn observed_nonless_is_not_ordered(
    fields: Seq<SchemaFieldId>,
    index: int,
    order: Ordering,
)
    requires
        1 <= index < fields.len(),
        order == crate::order::byte_order(
            fields[index - 1].spec_digest().spec_bytes()@,
            fields[index].spec_digest().spec_bytes()@,
        ),
        order != Ordering::Less,
    ensures !super::model::observed_fields_ordered(fields),
{
    if super::model::observed_fields_ordered(fields) {
        super::model::observed_ordered_at(fields, index);
    }
}

fn required_pair_order(fields: &[SchemaField], index: usize) -> (order: Ordering)
    requires 0 < index < fields@.len(),
    ensures order == crate::order::byte_order(
        fields@[index as int - 1].spec_id().spec_digest().spec_bytes()@,
        fields@[index as int].spec_id().spec_digest().spec_bytes()@,
    ),
{
    let previous = fields[index - 1].id();
    let current = fields[index].id();
    crate::order::compare(previous.digest().as_bytes(), current.digest().as_bytes())
}

fn observed_pair_order(fields: &[SchemaFieldId], index: usize) -> (order: Ordering)
    requires 0 < index < fields@.len(),
    ensures order == crate::order::byte_order(
        fields@[index as int - 1].spec_digest().spec_bytes()@,
        fields@[index as int].spec_digest().spec_bytes()@,
    ),
{
    crate::order::compare(
        fields[index - 1].digest().as_bytes(), fields[index].digest().as_bytes())
}

pub(super) fn validate_required_fields(
    fields: &[SchemaField],
    maximum: usize,
) -> (result: Result<(), ObligationError>)
    ensures result.is_ok() == (0 < fields@.len()
        && fields@.len() <= maximum
        && super::model::required_fields_ordered(fields@)),
        result.is_ok() ==> super::model::required_field_ids_unique(fields@),
{
    if fields.is_empty() || fields.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::InvalidSchema,
            maximum as u64,
            fields.len() as u64,
        ));
    }
    let mut index = 0;
    while index < fields.len()
        invariant
            0 < fields.len(),
            fields.len() <= maximum,
            index <= fields.len(),
            super::model::required_fields_ordered_through(fields@, index as nat),
        decreases fields.len() - index,
    {
        if index > 0 {
            let order = required_pair_order(fields, index);
            match order {
                Ordering::Equal => {
                    proof {
                        required_nonless_is_not_ordered(fields@, index as int, order);
                    }
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                Ordering::Greater => {
                    proof {
                        required_nonless_is_not_ordered(fields@, index as int, order);
                    }
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
                Ordering::Less => {}
            }
        }
        assert(super::model::required_fields_ordered_through(
            fields@, index as nat + 1)) by {
            assert forall |ordered_index: int| 1 <= ordered_index < index + 1 implies
                crate::order::byte_order(
                    #[trigger] fields@[ordered_index - 1].spec_id().spec_digest().spec_bytes()@,
                    #[trigger] fields@[ordered_index].spec_id().spec_digest().spec_bytes()@,
                ) == Ordering::Less by {
                if ordered_index < index {
                    super::model::required_ordered_through_at(
                        fields@, index as nat, ordered_index);
                } else {
                    assert(ordered_index == index);
                    if index == 0 { assert(false); }
                }
            }
        }
        index += 1;
    }
    assert(super::model::required_fields_ordered(fields@));
    proof { super::model::required_ordered_implies_unique(fields@); }
    Ok(())
}

pub(super) fn validate_observed_fields(
    fields: &[SchemaFieldId],
    maximum: usize,
) -> (result: Result<(), ObligationError>)
    ensures result.is_ok() == (fields@.len() <= maximum
        && super::model::observed_fields_ordered(fields@)),
        result.is_ok() ==> super::model::observed_field_ids_unique(fields@),
{
    if fields.len() > maximum {
        return Err(ObligationError::numbers(
            ObligationErrorKind::InvalidSchema,
            maximum as u64,
            fields.len() as u64,
        ));
    }
    let mut index = 0;
    while index < fields.len()
        invariant
            fields.len() <= maximum,
            index <= fields.len(),
            super::model::observed_fields_ordered_through(fields@, index as nat),
        decreases fields.len() - index,
    {
        if index > 0 {
            let order = observed_pair_order(fields, index);
            match order {
                Ordering::Equal => {
                    proof {
                        observed_nonless_is_not_ordered(fields@, index as int, order);
                    }
                    return Err(ObligationError::plain(ObligationErrorKind::DuplicateValue));
                }
                Ordering::Greater => {
                    proof {
                        observed_nonless_is_not_ordered(fields@, index as int, order);
                    }
                    return Err(ObligationError::plain(ObligationErrorKind::NonCanonicalOrder));
                }
                Ordering::Less => {}
            }
        }
        assert(super::model::observed_fields_ordered_through(
            fields@, index as nat + 1)) by {
            assert forall |ordered_index: int| 1 <= ordered_index < index + 1 implies
                crate::order::byte_order(
                    #[trigger] fields@[ordered_index - 1].spec_digest().spec_bytes()@,
                    #[trigger] fields@[ordered_index].spec_digest().spec_bytes()@,
                ) == Ordering::Less by {
                if ordered_index < index {
                    super::model::observed_ordered_through_at(
                        fields@, index as nat, ordered_index);
                } else {
                    assert(ordered_index == index);
                    if index == 0 { assert(false); }
                }
            }
        }
        index += 1;
    }
    assert(super::model::observed_fields_ordered(fields@));
    proof { super::model::observed_ordered_implies_unique(fields@); }
    Ok(())
}

} // verus!
