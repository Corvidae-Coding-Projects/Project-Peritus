use super::scanner::scan;
use super::violation::ViolationKind;

#[test]
fn rejects_public_safe_preconditions_but_accepts_internal_helpers() {
    let result = scan(
        r"
        verus! {
            fn private_helper(value: u64) requires value > 0 { }
            pub fn checked(value: u64) -> Result<(), ()> { Ok(()) }
            pub(crate) const fn crate_helper(value: u64)
                requires value > 0,
            { }
            pub(super) fn parent_helper(value: u64) requires value > 0 { }
            pub(in crate::model) fn scoped_helper(value: u64) requires value > 0 { }
            pub fn exposed(value: u64) requires value > 0 { }
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert_eq!(result.violations.len(), 1);
    assert_eq!(result.violations[0].function, "exposed");
    assert_eq!(result.violations[0].kind, ViolationKind::ExposedRequires);
}

#[test]
fn ignores_contract_words_in_comments_literals_and_function_bodies() {
    let result = scan(
        r#"
        // pub fn fake() requires false { }
        pub fn real() {
            let text = "requires";
            let requires = text.len() > 0;
            assert!(requires);
        }
        "#,
    );

    assert_eq!(result.executable_entry_points, 1);
    assert!(result.violations.is_empty(), "{:?}", result.violations);
}

#[test]
fn unsafe_functions_make_the_call_site_explicit() {
    let result = scan("pub unsafe fn unchecked(value: u64) requires value > 0 { }");

    assert_eq!(result.executable_entry_points, 1);
    assert!(result.violations.is_empty());
}

#[test]
fn rejects_contracts_unverified_implementers_could_violate() {
    let result = scan(
        r"
        pub trait Driver {
            fn read() -> (value: u64) ensures value > 0;
            fn stable() no_unwind { }
            fn open() opens_invariants none { }
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 3);
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::PublicTraitContract)
            .count(),
        3
    );
}

#[test]
fn rejects_safe_requires_even_on_unsafe_traits_and_trait_impls() {
    let result = scan(
        r"
        pub unsafe trait Driver {
            fn read(value: u64) requires value > 0;
        }
        unsafe impl Driver for Device {
            fn read(value: u64) requires value > 0 { }
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::ExposedRequires)
            .count(),
        2
    );
}

#[test]
fn unsafe_trait_methods_may_state_explicit_preconditions() {
    let result = scan(
        r"
        pub trait Driver {
            unsafe fn read(value: u64) requires value > 0;
        }
        impl Driver for Device {
            unsafe fn read(value: u64) requires value > 0 { }
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert!(result.violations.is_empty());
}

#[test]
fn inherent_impls_are_not_mistaken_for_trait_implementations() {
    let result = scan(
        r"
        impl<T> Container<T>
        where T: for<'a> Borrow<'a>
        {
            fn private(value: u64) requires value > 0 { }
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 0);
    assert!(result.violations.is_empty());
}

#[test]
fn rejects_opaque_returns_that_can_smuggle_private_preconditions() {
    let result = scan(
        r"
        fn closure() -> impl Fn(u64) {
            |value: u64| requires value > 0 { }
        }
        ",
    );

    assert_eq!(result.violations.len(), 1);
    assert_eq!(result.violations[0].kind, ViolationKind::OpaqueReturn);
}

#[test]
fn nested_generic_headers_are_delimited_before_the_body() {
    let result = scan(
        r"
        pub fn generic<T: Into<Vec<[u8; 16]>>>(value: T) -> Result<Vec<u8>, Error>
            ensures result.is_ok(),
        {
            consume(value)
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 1);
    assert!(result.violations.is_empty());
}

#[test]
fn malformed_public_headers_fail_closed() {
    let result = scan("pub fn incomplete");

    assert_eq!(result.executable_entry_points, 1);
    assert_eq!(result.violations.len(), 1);
    assert_eq!(result.violations[0].kind, ViolationKind::UnparseableHeader);
}

#[test]
fn rejects_attribute_form_preconditions_and_safe_trait_contracts() {
    let result = scan(
        r"
        #[verus_spec(ret => requires value > 0 ensures ret > 0)]
        pub fn checked(value: u64) -> u64 { value }

        pub trait Driver {
            #[verus_spec(ret => ensures ret > 0)]
            fn read() -> u64;
        }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert_eq!(result.violations.len(), 2);
    assert_eq!(result.violations[0].kind, ViolationKind::ExposedRequires);
    assert_eq!(result.violations[1].kind, ViolationKind::PublicTraitContract);
}

#[test]
fn unrelated_attributes_do_not_create_contracts() {
    let result = scan(
        r#"
        #[cfg(verus_only)]
        #[doc = "requires is documentation here"]
        pub fn checked(value: u64) -> u64 { value }
        "#,
    );

    assert_eq!(result.executable_entry_points, 1);
    assert!(result.violations.is_empty(), "{:?}", result.violations);
}

#[test]
fn rejects_conditional_qualified_and_unmodeled_expansions() {
    let result = scan(
        r"
        #[cfg_attr(verus_keep_ghost, verus_spec(requires false))]
        pub fn conditional() { let _ = 1_u8; }

        #[vstd::prelude::verus_spec(requires false)]
        pub fn qualified() { let _ = 2_u8; }

        #[generate_public_api]
        struct Input;

        generate_public_api! { Input }
        ",
    );

    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedAttribute)
            .count(),
        3
    );
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedMacro)
            .count(),
        1
    );
}

#[test]
fn permits_only_modeled_builtin_verus_and_trust_accounted_expansions() {
    let result = scan(
        r#"
        #![allow(dead_code)]
        #[derive(Clone, Copy)]
        #[cfg(verus_only)]
        struct Value;

        verus! {
            spec fn quantified(values: Seq<int>) -> bool {
                forall |index: int| #![auto]
                    0 <= index < values.len()
                        ==> #[trigger] values[index] == values[index]
            }

            #[verifier::type_invariant]
            spec fn invariant(&self) -> bool { true }

            #[external_body]
            fn modeled_boundary() { let _ = 3_u8; }
        }

        pub fn ordinary() {
            assert!(true);
            assert_ne!(1_u8, 2_u8);
            let _ = format!("{}", 1_u8);
            let _ = matches!(Some(1_u8), Some(_));
            let _ = ValueWithVector { values: vec![1_u8] };
        }

        #[path = "split.rs"]
        mod split;
        "#,
    );

    assert!(
        result.violations.iter().all(|violation| !matches!(
            violation.kind,
            ViolationKind::UnsupportedAttribute | ViolationKind::UnsupportedMacro
        )),
        "{:?}",
        result.violations
    );
}

#[test]
fn permits_only_the_exact_reviewed_serde_and_numeric_tag_forms() {
    let accepted = scan(
        r#"
        use serde::Deserialize;

        #[derive(Clone, Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input { value: u8 }

        #[derive(Copy, Clone, Deserialize)]
        #[serde(rename_all = "snake_case")]
        #[repr(u8)]
        enum Tag { First = 1 }
        "#,
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);

    for source in [
        "#[repr(C)] struct WrongRepresentation;",
        "#[serde(default)] struct Defaulted;",
        "use adversary::Deserialize; #[derive(Deserialize)] struct Shadowed;",
    ] {
        let rejected = scan(source);
        assert!(
            rejected.violations.iter().any(|violation| matches!(
                violation.kind,
                ViolationKind::UnsupportedAttribute | ViolationKind::UnsupportedMacro
            )),
            "{source}: {:?}",
            rejected.violations
        );
    }
}

#[test]
fn rejects_compile_environment_and_embedded_data_macros() {
    let result = scan(
        r#"
        const FROM_ENV: &str = env!("PERITUS_UNREVIEWED_INPUT");
        const TEXT: &str = include_str!("policy.txt");
        const BYTES: &[u8] = include_bytes!("policy.bin");
        "#,
    );

    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedMacro)
            .count(),
        3
    );
}

#[test]
fn permits_only_audited_cargo_environment_macros() {
    let result = scan(
        r#"
        const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");
        const CLAUDE: &str = env!("CARGO_BIN_EXE_peritus-anthropic-claude-fake");
        const CODEX: &str = env!("CARGO_BIN_EXE_peritus-openai-codex-fake");
        const UNREVIEWED: &str = env!("CARGO_BIN_EXE_unreviewed-helper");
        "#,
    );

    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedMacro)
            .count(),
        1
    );
}

#[test]
fn rejects_custom_derives_and_external_expansion_imports() {
    let result = scan(
        r"
        use external_macros::evil as assert;
        use external_macros::evil as assert_ne;
        use external_macros::evil as auto;
        use external_macros::evil as matches;
        use external_macros::evil as trigger;
        use external_macros::*;
        use crate::reexported::evil as assert_eq;

        #[derive(Clone, external_macros::Contract)]
        struct Qualified;

        #[derive(Clone, Contract)]
        struct Custom;
        ",
    );

    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedAttribute)
            .count(),
        2
    );
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::UnsupportedMacro)
            .count(),
        7
    );
}
