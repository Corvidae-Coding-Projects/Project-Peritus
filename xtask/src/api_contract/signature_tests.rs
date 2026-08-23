use super::scanner::scan;
use super::violation::ViolationKind;

#[test]
fn proof_and_spec_functions_are_not_ordinary_entry_points() {
    let result = scan(
        r"
        pub closed spec fn model(value: int) -> bool recommends value > 0 { true }
        pub proof fn lemma(value: int) requires value > 0 { }
        ",
    );

    assert_eq!(result.executable_entry_points, 0);
    assert!(result.violations.is_empty());
}

#[test]
fn mode_identifiers_outside_the_modifier_sequence_cannot_hide_preconditions() {
    let result = scan(
        r"
        #[verus_spec(requires proof)]
        pub fn unchecked(proof: bool) { let _ = proof; }
        #[verus_spec(requires spec)]
        pub fn unchecked_spec(spec: bool) { let _ = spec; }
        #[cfg(proof)]
        pub fn configured(value: u64) requires value > 0 { let _ = value; }
        #[allow(spec)]
        pub fn allowed(value: u64) requires value > 0 { let _ = value; }
        pub(in proof) fn proof_scope(value: u64) requires value > 0 { let _ = value; }
        pub(in spec) fn spec_scope(value: u64) requires value > 0 { let _ = value; }
        ",
    );

    assert_eq!(result.executable_entry_points, 4);
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::ExposedRequires)
            .count(),
        4
    );
}

#[test]
fn recognizes_the_structural_verus_function_modifier_grammar() {
    let result = scan(
        r"
        pub open spec fn open_spec() -> bool { true }
        pub open(crate) spec fn restricted_spec() -> bool { true }
        pub open(in crate::model) spec fn path_spec() -> bool { true }
        pub closed spec fn closed_spec() -> bool { true }
        pub uninterp spec fn uninterpreted() -> bool;
        pub spec(checked) fn checked_spec() -> bool { true }
        pub proof fn lemma() { assert(true); }
        pub broadcast proof fn broadcast_lemma() { assert(true); }
        pub axiom fn axiom_lemma();
        pub broadcast axiom fn broadcast_axiom();
        pub exec fn explicit_exec() { let _ = 1_u8; }
        pub const fn implicit_exec() { let _ = 2_u8; }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert!(result.violations.is_empty(), "{:?}", result.violations);
}

#[test]
fn rejects_malformed_or_out_of_order_function_modifiers() {
    let result = scan(
        r"
        pub proof const fn proof_before_const(value: u64) requires value > 0 { let _ = value; }
        pub unsafe async fn unsafe_before_async(value: u64) { let _ = value; }
        pub exec broadcast fn mode_before_broadcast(value: u64) { let _ = value; }
        pub spec(other) fn unsupported_spec_mode() -> bool { true }
        pub extern 7 fn malformed_abi() { let _ = 1_u8; }
        ",
    );

    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnparseableHeader)
            .count(),
        5
    );
}

#[test]
fn trait_context_uses_structural_modifiers_not_attribute_payloads() {
    let result = scan(
        r"
        #[cfg(unsafe)]
        pub trait SafeDriver {
            fn read() -> (value: u64) ensures value > 0;
        }

        #[cfg(pub)]
        trait PrivateDriver {
            fn private() -> (value: u64) ensures value > 0;
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 1);
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::PublicTraitContract)
            .count(),
        1
    );
}
