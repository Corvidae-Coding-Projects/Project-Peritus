//! Executable predicates shared with Verus proof obligations.

#[doc = " Scalar projection of independently checked envelope bounds."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "formal projection keeps every independent bound check visible"
)]
pub struct ProtocolBoundFacts {
    #[doc = " Canonical bytes fit their envelope limit."]
    pub bytes_fit: bool,
    #[doc = " Recursive depth fits its limit."]
    pub depth_fits: bool,
    #[doc = " Total members fit their limit."]
    pub members_fit: bool,
    #[doc = " Every string fits its limit."]
    pub strings_fit: bool,
    #[doc = " Progress count fits the call limit."]
    pub progress_fits: bool,
    #[doc = " Artifact count fits the call limit."]
    pub artifacts_fit: bool,
}
#[doc = " Checks that every independently enforced C4 envelope bound is present."]
#[must_use]
pub const fn protocol_bounds_complete(facts: ProtocolBoundFacts) -> bool {
    facts.bytes_fit
        && facts.depth_fits
        && facts.members_fit
        && facts.strings_fit
        && facts.progress_fits
        && facts.artifacts_fit
}
#[doc = " Checks canonical-order evidence for schemas/descriptors/enumerations."]
#[must_use]
pub const fn canonical_order_complete(strictly_increasing: bool, duplicate_free: bool) -> bool {
    strictly_increasing && duplicate_free
}
#[doc = " Checks every independently validated schema-shape dimension."]
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
) -> bool {
    supported_type && cardinality_valid && children_valid && enumeration_valid
}
