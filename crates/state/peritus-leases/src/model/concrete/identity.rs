//! Exact nominal identity views used by executable validation refinement.

#[doc = " Compares all sixteen bytes of two nominal identifier representations."]
const fn identifier_values_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}
#[doc = " Compares complete fixed-width nominal identifier representations."]
pub const fn identifier_values_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    identifier_values_equal_from(left, right, 0)
}
#[doc = " Compares every nominal identifier dimension of two lease scopes."]
pub const fn lease_scopes_equal(left: crate::LeaseScope, right: crate::LeaseScope) -> bool {
    if !identifier_values_equal(*left.workspace.as_bytes(), *right.workspace.as_bytes()) {
        return false;
    }
    if !identifier_values_equal(*left.resource.as_bytes(), *right.resource.as_bytes()) {
        return false;
    }
    identifier_values_equal(*left.environment.as_bytes(), *right.environment.as_bytes())
}
