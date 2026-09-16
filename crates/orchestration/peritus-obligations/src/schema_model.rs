//! Exact logical views of directional schema requirements and observations.

#[cfg(verus_only)]
use super::SchemaFieldId;
use super::{SchemaDirection, SchemaEvidence, SchemaField, SchemaRequirement};
use vstd::prelude::*;

verus! {

/// Complete field-identity bytes in caller-supplied order.
pub open spec fn required_field_keys(fields: Seq<SchemaField>) -> Seq<Seq<u8>> {
    fields.map(|_index: int, field: SchemaField| field.spec_id().spec_digest().spec_bytes()@)
}

/// Complete observed field-identity bytes in caller-supplied order.
pub open spec fn observed_field_keys(fields: Seq<SchemaFieldId>) -> Seq<Seq<u8>> {
    fields.map(|_index: int, field: SchemaFieldId| field.spec_digest().spec_bytes()@)
}

/// Strict canonical ordering of required field identities.
pub open spec fn required_fields_ordered(fields: Seq<SchemaField>) -> bool {
    forall |index: int| 1 <= index < fields.len() ==>
        crate::order::byte_order(
            #[trigger] fields[index - 1].spec_id().spec_digest().spec_bytes()@,
            #[trigger] fields[index].spec_id().spec_digest().spec_bytes()@,
        ) == core::cmp::Ordering::Less
}

/// Strict canonical ordering through one required-field prefix.
pub open spec fn required_fields_ordered_through(
    fields: Seq<SchemaField>,
    end: nat,
) -> bool {
    end <= fields.len()
        && forall |index: int| 1 <= index < end ==>
            crate::order::byte_order(
                #[trigger] fields[index - 1].spec_id().spec_digest().spec_bytes()@,
                #[trigger] fields[index].spec_id().spec_digest().spec_bytes()@,
            ) == core::cmp::Ordering::Less
}

/// Strict canonical ordering of observed field identities.
pub open spec fn observed_fields_ordered(fields: Seq<SchemaFieldId>) -> bool {
    forall |index: int| 1 <= index < fields.len() ==>
        crate::order::byte_order(
            #[trigger] fields[index - 1].spec_digest().spec_bytes()@,
            #[trigger] fields[index].spec_digest().spec_bytes()@,
        ) == core::cmp::Ordering::Less
}

/// Strict canonical ordering through one observed-field prefix.
pub open spec fn observed_fields_ordered_through(
    fields: Seq<SchemaFieldId>,
    end: nat,
) -> bool {
    end <= fields.len()
        && forall |index: int| 1 <= index < end ==>
            crate::order::byte_order(
                #[trigger] fields[index - 1].spec_digest().spec_bytes()@,
                #[trigger] fields[index].spec_digest().spec_bytes()@,
            ) == core::cmp::Ordering::Less
}

/// Global uniqueness of every required field identity byte sequence.
pub open spec fn required_field_ids_unique(fields: Seq<SchemaField>) -> bool {
    forall |left: int, right: int| 0 <= left < right < fields.len() ==>
        #[trigger] fields[left].spec_id().spec_digest().spec_bytes()@
            != #[trigger] fields[right].spec_id().spec_digest().spec_bytes()@
}

/// Global uniqueness of every observed field identity byte sequence.
pub open spec fn observed_field_ids_unique(fields: Seq<SchemaFieldId>) -> bool {
    forall |left: int, right: int| 0 <= left < right < fields.len() ==>
        #[trigger] fields[left].spec_digest().spec_bytes()@
            != #[trigger] fields[right].spec_digest().spec_bytes()@
}

/// Strictly ordered required fields are globally unique.
pub proof fn required_ordered_implies_unique(fields: Seq<SchemaField>)
    requires required_fields_ordered(fields),
    ensures required_field_ids_unique(fields),
{
    assert(crate::order::ordered(required_field_keys(fields))) by {
        assert forall |index: int| 1 <= index < fields.len() implies
            crate::order::byte_order(
                #[trigger] required_field_keys(fields)[index - 1],
                #[trigger] required_field_keys(fields)[index],
            ) == core::cmp::Ordering::Less by {
            assert(required_field_keys(fields)[index - 1]
                == fields[index - 1].spec_id().spec_digest().spec_bytes()@);
            assert(required_field_keys(fields)[index]
                == fields[index].spec_id().spec_digest().spec_bytes()@);
        }
    }
    assert forall |index: int| 0 <= index < required_field_keys(fields).len() implies
        #[trigger] required_field_keys(fields)[index].len() == 32 by {
        assert(required_field_keys(fields)[index]
            == fields[index].spec_id().spec_digest().spec_bytes()@);
    }
    crate::order::ordered_implies_unique(required_field_keys(fields), 32);
    assert forall |left: int, right: int| 0 <= left < right < fields.len() implies
        #[trigger] fields[left].spec_id().spec_digest().spec_bytes()@
            != #[trigger] fields[right].spec_id().spec_digest().spec_bytes()@ by {
        assert(required_field_keys(fields)[left]
            == fields[left].spec_id().spec_digest().spec_bytes()@);
        assert(required_field_keys(fields)[right]
            == fields[right].spec_id().spec_digest().spec_bytes()@);
    }
}

/// Strictly ordered observed fields are globally unique.
pub proof fn observed_ordered_implies_unique(fields: Seq<SchemaFieldId>)
    requires observed_fields_ordered(fields),
    ensures observed_field_ids_unique(fields),
{
    assert(crate::order::ordered(observed_field_keys(fields))) by {
        assert forall |index: int| 1 <= index < fields.len() implies
            crate::order::byte_order(
                #[trigger] observed_field_keys(fields)[index - 1],
                #[trigger] observed_field_keys(fields)[index],
            ) == core::cmp::Ordering::Less by {
            assert(observed_field_keys(fields)[index - 1]
                == fields[index - 1].spec_digest().spec_bytes()@);
            assert(observed_field_keys(fields)[index]
                == fields[index].spec_digest().spec_bytes()@);
        }
    }
    assert forall |index: int| 0 <= index < observed_field_keys(fields).len() implies
        #[trigger] observed_field_keys(fields)[index].len() == 32 by {
        assert(observed_field_keys(fields)[index]
            == fields[index].spec_digest().spec_bytes()@);
    }
    crate::order::ordered_implies_unique(observed_field_keys(fields), 32);
    assert forall |left: int, right: int| 0 <= left < right < fields.len() implies
        #[trigger] fields[left].spec_digest().spec_bytes()@
            != #[trigger] fields[right].spec_digest().spec_bytes()@ by {
        assert(observed_field_keys(fields)[left]
            == fields[left].spec_digest().spec_bytes()@);
        assert(observed_field_keys(fields)[right]
            == fields[right].spec_digest().spec_bytes()@);
    }
}

/// Every required field identity is present in the observed identity set.
pub open spec fn all_required_fields_present(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
) -> bool {
    forall |required_index: int| 0 <= required_index < required.len() ==>
        #[trigger] required_field_present(required, observed, required_index)
}

/// Exact field membership for one required position.
pub open spec fn required_field_present(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
) -> bool {
    exists |observed_index: int| 0 <= observed_index < observed.len()
        && #[trigger] observed[observed_index].spec_digest().spec_bytes()@
            == required[required_index].spec_id().spec_digest().spec_bytes()@
}

/// Every required field before one prefix end is present in the observation.
pub open spec fn required_fields_present_through(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    end: nat,
) -> bool {
    end <= required.len()
        && forall |required_index: int| 0 <= required_index < end ==>
            required_field_present(required, observed, required_index)
}

/// Every observed field before one cursor sorts before the current requirement.
pub open spec fn observed_before_required(
    required: Seq<SchemaField>,
    observed: Seq<SchemaFieldId>,
    required_index: int,
    observed_end: nat,
) -> bool {
    0 <= required_index < required.len()
        && observed_end <= observed.len()
        && forall |observed_index: int| 0 <= observed_index < observed_end ==>
            crate::order::byte_order(
                #[trigger] observed[observed_index].spec_digest().spec_bytes()@,
                required[required_index].spec_id().spec_digest().spec_bytes()@,
            ) == core::cmp::Ordering::Less
}

/// Selects one adjacent required-field order fact from the canonical invariant.
pub proof fn required_ordered_at(fields: Seq<SchemaField>, index: int)
    requires required_fields_ordered(fields), 1 <= index < fields.len(),
    ensures crate::order::byte_order(
        fields[index - 1].spec_id().spec_digest().spec_bytes()@,
        fields[index].spec_id().spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
}

/// Selects one required-field order fact from a checked prefix.
pub proof fn required_ordered_through_at(
    fields: Seq<SchemaField>,
    end: nat,
    index: int,
)
    requires required_fields_ordered_through(fields, end), 1 <= index < end,
    ensures crate::order::byte_order(
        fields[index - 1].spec_id().spec_digest().spec_bytes()@,
        fields[index].spec_id().spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
}

/// Selects one adjacent observed-field order fact from the canonical invariant.
pub proof fn observed_ordered_at(fields: Seq<SchemaFieldId>, index: int)
    requires observed_fields_ordered(fields), 1 <= index < fields.len(),
    ensures crate::order::byte_order(
        fields[index - 1].spec_digest().spec_bytes()@,
        fields[index].spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
}

/// Selects one observed-field order fact from a checked prefix.
pub proof fn observed_ordered_through_at(
    fields: Seq<SchemaFieldId>,
    end: nat,
    index: int,
)
    requires observed_fields_ordered_through(fields, end), 1 <= index < end,
    ensures crate::order::byte_order(
        fields[index - 1].spec_digest().spec_bytes()@,
        fields[index].spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
}

/// Any two increasing required-field positions are strictly byte ordered.
pub proof fn required_ordered_pair(
    fields: Seq<SchemaField>,
    left: int,
    right: int,
)
    requires required_fields_ordered(fields), 0 <= left < right < fields.len(),
    ensures crate::order::byte_order(
        fields[left].spec_id().spec_digest().spec_bytes()@,
        fields[right].spec_id().spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
    assert(crate::order::ordered(required_field_keys(fields))) by {
        assert forall |index: int| 1 <= index < fields.len() implies
            crate::order::byte_order(
                #[trigger] required_field_keys(fields)[index - 1],
                #[trigger] required_field_keys(fields)[index],
            ) == core::cmp::Ordering::Less by {
            required_ordered_at(fields, index);
        }
    }
    assert forall |index: int| 0 <= index < required_field_keys(fields).len() implies
        #[trigger] required_field_keys(fields)[index].len() == 32 by {
        assert(required_field_keys(fields)[index]
            == fields[index].spec_id().spec_digest().spec_bytes()@);
    }
    crate::order::ordered_pair(required_field_keys(fields), 32, left, right);
}

/// Any two increasing observed-field positions are strictly byte ordered.
pub proof fn observed_ordered_pair(
    fields: Seq<SchemaFieldId>,
    left: int,
    right: int,
)
    requires observed_fields_ordered(fields), 0 <= left < right < fields.len(),
    ensures crate::order::byte_order(
        fields[left].spec_digest().spec_bytes()@,
        fields[right].spec_digest().spec_bytes()@,
    ) == core::cmp::Ordering::Less,
{
    assert(crate::order::ordered(observed_field_keys(fields))) by {
        assert forall |index: int| 1 <= index < fields.len() implies
            crate::order::byte_order(
                #[trigger] observed_field_keys(fields)[index - 1],
                #[trigger] observed_field_keys(fields)[index],
            ) == core::cmp::Ordering::Less by {
            observed_ordered_at(fields, index);
        }
    }
    assert forall |index: int| 0 <= index < observed_field_keys(fields).len() implies
        #[trigger] observed_field_keys(fields)[index].len() == 32 by {
        assert(observed_field_keys(fields)[index]
            == fields[index].spec_digest().spec_bytes()@);
    }
    crate::order::ordered_pair(observed_field_keys(fields), 32, left, right);
}

impl SchemaDirection {
    /// Exact executable equality for schema directions.
    pub(super) const fn same(self, other: Self) -> (same: bool)
        ensures same == (self == other),
    {
        matches!((self, other), (Self::Request, Self::Request) | (Self::Response, Self::Response))
    }
}

impl SchemaField {
    /// Intrinsic field invariant retained after the constructor forgets its caller bound.
    pub open spec fn spec_valid(&self) -> bool { 0 < self.spec_exact_name().len() }

    /// Complete semantic equality of a schema field.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_id() == other.spec_id()
            && self.spec_exact_name() == other.spec_exact_name()
    }

    /// Pairwise semantic equality of two field sequences.
    pub open spec fn sequence_same_content(
        left: Seq<SchemaField>,
        right: Seq<SchemaField>,
    ) -> bool {
        left.len() == right.len()
            && forall |index: int| 0 <= index < left.len() ==>
                #[trigger] left[index].spec_same_content(&right[index])
    }
}

impl SchemaRequirement {
    /// Nonempty canonical required-field identity set.
    pub open spec fn spec_canonical(&self) -> bool {
        0 < self.spec_fields().len()
            && required_fields_ordered(self.spec_fields())
            && required_field_ids_unique(self.spec_fields())
    }

    /// Complete semantic equality of a schema requirement.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_direction() == other.spec_direction()
            && SchemaField::sequence_same_content(self.spec_fields(), other.spec_fields())
    }
}

impl SchemaEvidence {
    /// Canonical observed-field identity set.
    pub open spec fn spec_canonical(&self) -> bool {
        observed_fields_ordered(self.spec_observed_fields())
            && observed_field_ids_unique(self.spec_observed_fields())
    }

    /// Exact direction and all-required-field membership semantics.
    pub open spec fn spec_covers(&self, requirement: &SchemaRequirement) -> bool {
        self.spec_direction() == requirement.spec_direction()
            && all_required_fields_present(
                requirement.spec_fields(), self.spec_observed_fields())
    }

    /// Complete semantic equality of schema evidence.
    pub open spec fn spec_same_content(&self, other: &Self) -> bool {
        self.spec_binding().spec_same_content(&other.spec_binding())
            && self.spec_direction() == other.spec_direction()
            && self.spec_observed_fields() == other.spec_observed_fields()
    }
}

} // verus!
