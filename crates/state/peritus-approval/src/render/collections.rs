//! Whole-item rendering and exact omission accounting for bounded collections.

use super::builder::Builder;
use super::encoding::hex;
use super::{MAX_RENDERED_PARTICIPANTS, MAX_RENDERED_PERMISSIONS};
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy)]
pub(super) struct CollectionCounts {
    pub(super) permission_total: usize,
    pub(super) permission_omitted: usize,
    pub(super) producing_total: usize,
    pub(super) producing_omitted: usize,
    pub(super) review_total: usize,
    pub(super) review_omitted: usize,
}

fn participants(
    builder: &mut Builder,
    field_name: &str,
    values: &[peritus_types::ActorId],
) -> Result<usize, crate::ApprovalError> {
    let attempted = values.len().min(MAX_RENDERED_PARTICIPANTS);
    let mut omitted = values.len() - attempted;
    let mut index = 0;
    while index < attempted
        invariant
            0 <= index <= attempted <= values.len(),
            omitted <= values.len(),
            omitted <= values.len() - attempted + index,
        decreases attempted - index,
    {
        if !builder.field(field_name, &hex(values[index].as_bytes()))? {
            assert(omitted < values.len());
            omitted += 1;
        }
        index += 1;
    }
    Ok(omitted)
}

fn permissions(
    builder: &mut Builder,
    values: &[peritus_policy::Permission],
) -> Result<usize, crate::ApprovalError> {
    let attempted = values.len().min(MAX_RENDERED_PERMISSIONS);
    let mut omitted = values.len() - attempted;
    let mut index = 0;
    while index < attempted
        invariant
            0 <= index <= attempted <= values.len(),
            omitted <= values.len(),
            omitted <= values.len() - attempted + index,
        decreases attempted - index,
    {
        let mut value = hex(values[index].resource_id().as_bytes());
        value.push(':');
        value.push_str(values[index].capability_name().as_str());
        if !builder.field("permission", &value)? {
            assert(omitted < values.len());
            omitted += 1;
        }
        index += 1;
    }
    Ok(omitted)
}

pub(super) fn render(
    builder: &mut Builder,
    request: &crate::ApprovalRequest,
) -> Result<CollectionCounts, crate::ApprovalError> {
    let producing = request.producing_participants().as_slice();
    let review = request.review_participants().as_slice();
    let permission_values = request.scope().permissions().as_slice();
    let producing_omitted = participants(builder, "producing-participant", producing)?;
    let review_omitted = participants(builder, "review-participant", review)?;
    let permission_omitted = permissions(builder, permission_values)?;
    if producing_omitted > 0 || review_omitted > 0 || permission_omitted > 0 {
        builder.truncated = true;
    }
    Ok(CollectionCounts {
        permission_total: permission_values.len(),
        permission_omitted,
        producing_total: producing.len(),
        producing_omitted,
        review_total: review.len(),
        review_omitted,
    })
}

} // verus!
