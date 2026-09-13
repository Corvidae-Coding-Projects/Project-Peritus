//! Exact linear subset matching for canonical schema field identities.

use super::{SchemaEvidence, SchemaField, SchemaFieldId, SchemaRequirement};
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

pub(super) open spec fn required_key(fields: Seq<SchemaField>, index: int) -> Seq<u8> {
    fields[index].spec_id().spec_digest().spec_bytes()@
}

pub(super) open spec fn observed_key(fields: Seq<SchemaFieldId>, index: int) -> Seq<u8> {
    fields[index].spec_digest().spec_bytes()@
}

fn compare_at(
    required: &[SchemaField],
    required_index: usize,
    observed: &[SchemaFieldId],
    observed_index: usize,
) -> (order: Ordering)
    requires required_index < required@.len(), observed_index < observed@.len(),
    ensures order == crate::order::byte_order(
        observed_key(observed@, observed_index as int),
        required_key(required@, required_index as int),
    ),
        (order == Ordering::Equal) == (
            observed_key(observed@, observed_index as int)
                == required_key(required@, required_index as int)),
{
    crate::order::compare(
        observed[observed_index].digest().as_bytes(),
        required[required_index].id().digest().as_bytes(),
    )
}

proof fn current_is_present(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_index: int,
)
    requires
        0 <= required_index < required.len(),
        0 <= observed_index < observed.len(),
        observed_key(observed, observed_index) == required_key(required, required_index),
    ensures super::model::required_field_present(required, observed, required_index),
{
    assert(exists |found: int| 0 <= found < observed.len()
        && #[trigger] observed[found].spec_digest().spec_bytes()@
            == required[required_index].spec_id().spec_digest().spec_bytes()@) by {
        let found = observed_index;
    }
}

proof fn extend_present_prefix(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: nat,
)
    requires
        required_index < required.len(),
        super::model::required_fields_present_through(required, observed, required_index),
        super::model::required_field_present(required, observed, required_index as int),
    ensures super::model::required_fields_present_through(
        required, observed, required_index + 1),
{
    assert forall |index: int| 0 <= index < required_index + 1 implies
        super::model::required_field_present(required, observed, index) by {
        if index < required_index {
            assert(super::model::required_fields_present_through(
                required, observed, required_index));
        } else {
            assert(index == required_index);
        }
    }
}

proof fn missing_breaks_all(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
)
    requires
        0 <= required_index < required.len(),
        !super::model::required_field_present(required, observed, required_index),
    ensures !super::model::all_required_fields_present(required, observed),
{
    if super::model::all_required_fields_present(required, observed) {
        assert(super::model::required_field_present(required, observed, required_index));
    }
}

proof fn exhausted_observation_misses_current(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_end: nat,
)
    requires
        super::model::observed_before_required(
            required, observed, required_index, observed_end),
        observed_end == observed.len(),
    ensures !super::model::required_field_present(required, observed, required_index),
{
    if super::model::required_field_present(required, observed, required_index) {
        let found = choose |found: int| 0 <= found < observed.len()
            && #[trigger] observed[found].spec_digest().spec_bytes()@
                == required[required_index].spec_id().spec_digest().spec_bytes()@;
        assert(found < observed_end);
        assert(crate::order::byte_order(
            observed_key(observed, found), required_key(required, required_index),
        ) == Ordering::Less);
        crate::order::byte_order_reflexive(required_key(required, required_index));
        assert(false);
    }
}

proof fn greater_observation_misses_current(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_index: int,
)
    requires
        super::model::observed_fields_ordered(observed),
        0 <= observed_index,
        super::model::observed_before_required(
            required, observed, required_index, observed_index as nat),
        observed_index < observed.len(),
        crate::order::byte_order(
            observed_key(observed, observed_index),
            required_key(required, required_index),
        ) == Ordering::Greater,
    ensures !super::model::required_field_present(required, observed, required_index),
{
    if super::model::required_field_present(required, observed, required_index) {
        let found = choose |found: int| 0 <= found < observed.len()
            && #[trigger] observed[found].spec_digest().spec_bytes()@
                == required[required_index].spec_id().spec_digest().spec_bytes()@;
        if found < observed_index {
            assert(crate::order::byte_order(
                observed_key(observed, found), required_key(required, required_index),
            ) == Ordering::Less);
        } else if found == observed_index {
        } else {
            super::model::observed_ordered_pair(observed, observed_index, found);
        }
        crate::order::byte_order_reflexive(required_key(required, required_index));
        assert(false);
    }
}

proof fn extend_observed_prefix(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_index: nat,
)
    requires
        super::model::observed_before_required(
            required, observed, required_index, observed_index),
        observed_index < observed.len(),
        crate::order::byte_order(
            observed_key(observed, observed_index as int),
            required_key(required, required_index),
        ) == Ordering::Less,
    ensures super::model::observed_before_required(
        required, observed, required_index, observed_index + 1),
{
    assert forall |index: int| 0 <= index < observed_index + 1 implies
        crate::order::byte_order(
            #[trigger] observed[index].spec_digest().spec_bytes()@,
            required[required_index].spec_id().spec_digest().spec_bytes()@,
        ) == Ordering::Less by {
        if index < observed_index {
            assert(super::model::observed_before_required(
                required, observed, required_index, observed_index));
        } else {
            assert(index == observed_index);
        }
    }
}

proof fn advance_to_next_requirement(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_index: nat,
)
    requires
        super::model::required_fields_ordered(required),
        super::model::observed_before_required(
            required, observed, required_index, observed_index),
        required_index + 1 < required.len(),
        observed_index < observed.len(),
        observed_key(observed, observed_index as int)
            == required_key(required, required_index),
    ensures super::model::observed_before_required(
        required, observed, required_index + 1, observed_index + 1),
{
    super::model::required_ordered_at(required, required_index + 1);
    assert forall |index: int| 0 <= index < observed_index + 1 implies
        crate::order::byte_order(
            #[trigger] observed[index].spec_digest().spec_bytes()@,
            required[required_index + 1].spec_id().spec_digest().spec_bytes()@,
        ) == Ordering::Less by {
        if index < observed_index {
            crate::order::less_transitive(
                observed_key(observed, index),
                required_key(required, required_index),
                required_key(required, required_index + 1),
            );
        } else {
            assert(index == observed_index);
        }
    }
}

proof fn completed_prefix_is_all(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
)
    requires super::model::required_fields_present_through(
        required, observed, required.len()),
    ensures super::model::all_required_fields_present(required, observed),
{
    assert forall |index: int| 0 <= index < required.len() implies
        super::model::required_field_present(required, observed, index) by {
        assert(super::model::required_fields_present_through(
            required, observed, required.len()));
    }
}

pub(super) fn covers(
    evidence: &SchemaEvidence,
    requirement: &SchemaRequirement,
) -> (covered: bool)
    ensures covered == evidence.spec_covers(requirement),
{
    proof { use_type_invariant(evidence); use_type_invariant(requirement); }
    if !evidence.direction.same(requirement.direction()) { return false; }
    let required = requirement.fields();
    let observed = evidence.observed_fields();
    let mut required_index = 0;
    let mut observed_index = 0;
    while required_index < required.len()
        invariant
            required@ == requirement.spec_fields(),
            observed@ == evidence.spec_observed_fields(),
            evidence.spec_direction() == requirement.spec_direction(),
            requirement.spec_canonical(), evidence.spec_canonical(),
            required_index <= required@.len(), observed_index <= observed@.len(),
            super::model::required_fields_present_through(
                required@, observed@, required_index as nat),
            required_index < required@.len() ==> super::model::observed_before_required(
                required@, observed@, required_index as int, observed_index as nat),
        decreases (required@.len() - required_index) + (observed@.len() - observed_index),
    {
        if observed_index >= observed.len() {
            proof {
                exhausted_observation_misses_current(
                    required@, observed@, required_index as int, observed_index as nat);
                missing_breaks_all(required@, observed@, required_index as int);
            }
            return false;
        }
        let order = compare_at(required, required_index, observed, observed_index);
        match order {
            Ordering::Equal => {
                proof {
                    current_is_present(
                        required@, observed@, required_index as int, observed_index as int);
                    extend_present_prefix(required@, observed@, required_index as nat);
                    if required_index + 1 < required.len() {
                        advance_to_next_requirement(
                            required@, observed@, required_index as int, observed_index as nat);
                    }
                }
                required_index += 1;
                observed_index += 1;
            }
            Ordering::Less => {
                proof {
                    extend_observed_prefix(
                        required@, observed@, required_index as int, observed_index as nat);
                }
                observed_index += 1;
            }
            Ordering::Greater => {
                proof {
                    greater_observation_misses_current(
                        required@, observed@, required_index as int, observed_index as int);
                    missing_breaks_all(required@, observed@, required_index as int);
                }
                return false;
            }
        }
    }
    proof { completed_prefix_is_all(required@, observed@); }
    true
}

} // verus!
