//! Executable predicates shared with Verus proof obligations.

use vstd::prelude::*;

verus! {

/// Scalar projection of independently checked envelope bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal projection keeps every independent bound check visible"
)]
pub struct ProtocolBoundFacts {
    /// Canonical bytes fit their envelope limit.
    pub bytes_fit: bool,
    /// Recursive depth fits its limit.
    pub depth_fits: bool,
    /// Total members fit their limit.
    pub members_fit: bool,
    /// Every string fits its limit.
    pub strings_fit: bool,
    /// Progress count fits the call limit.
    pub progress_fits: bool,
    /// Artifact count fits the call limit.
    pub artifacts_fit: bool,
}

/// Mathematical complete C4 bound predicate.
pub open spec fn protocol_bounds_complete_spec(facts: ProtocolBoundFacts) -> bool {
    facts.bytes_fit
        && facts.depth_fits
        && facts.members_fit
        && facts.strings_fit
        && facts.progress_fits
        && facts.artifacts_fit
}

/// Checks that every independently enforced C4 envelope bound is present.
#[must_use]
pub const fn protocol_bounds_complete(facts: ProtocolBoundFacts) -> (result: bool)
    ensures result == protocol_bounds_complete_spec(facts),
{
    facts.bytes_fit
        && facts.depth_fits
        && facts.members_fit
        && facts.strings_fit
        && facts.progress_fits
        && facts.artifacts_fit
}

/// Mathematical canonical adjacent-order predicate.
pub open spec fn canonical_order_complete_spec(
    strictly_increasing: bool,
    duplicate_free: bool,
) -> bool {
    strictly_increasing && duplicate_free
}

/// Checks canonical-order evidence for schemas/descriptors/enumerations.
#[must_use]
pub const fn canonical_order_complete(
    strictly_increasing: bool,
    duplicate_free: bool,
) -> (result: bool)
    ensures result == canonical_order_complete_spec(strictly_increasing, duplicate_free),
{
    strictly_increasing && duplicate_free
}

/// Mathematical supported-schema shape predicate.
pub open spec fn schema_shape_complete_spec(
    supported_type: bool,
    cardinality_valid: bool,
    children_valid: bool,
    enumeration_valid: bool,
) -> bool {
    supported_type && cardinality_valid && children_valid && enumeration_valid
}

/// Checks every independently validated schema-shape dimension.
#[must_use]
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "formal projection keeps every schema check independently visible"
)]
pub const fn schema_shape_complete(
    supported_type: bool,
    cardinality_valid: bool,
    children_valid: bool,
    enumeration_valid: bool,
) -> (result: bool)
    ensures result == schema_shape_complete_spec(
        supported_type,
        cardinality_valid,
        children_valid,
        enumeration_valid,
    ),
{
    supported_type && cardinality_valid && children_valid && enumeration_valid
}

} // verus!

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_reject_each_missing_fact() {
        let exact = ProtocolBoundFacts {
            bytes_fit: true,
            depth_fits: true,
            members_fit: true,
            strings_fit: true,
            progress_fits: true,
            artifacts_fit: true,
        };
        assert!(protocol_bounds_complete(exact));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { bytes_fit: false, ..exact }));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { depth_fits: false, ..exact }));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { members_fit: false, ..exact }));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { strings_fit: false, ..exact }));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { progress_fits: false, ..exact }));
        assert!(!protocol_bounds_complete(ProtocolBoundFacts { artifacts_fit: false, ..exact }));
    }
}
