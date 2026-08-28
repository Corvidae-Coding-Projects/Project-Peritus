use super::scanner::scan;
use super::violation::ViolationKind;

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
        use serde::Serialize;

        #[derive(Clone, Debug, Deserialize, Serialize)]
        #[serde(deny_unknown_fields)]
        struct Input { value: u8 }

        #[derive(Copy, Clone, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        #[repr(u8)]
        enum Tag { First = 1 }

        #[derive(Deserialize)]
        struct Compatible { #[serde(default)] value: u8 }
        "#,
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);

    for source in [
        "#[repr(C)] struct WrongRepresentation;",
        "#[serde(rename_all = \"SCREAMING_SNAKE_CASE\")] enum WrongCase { Value }",
        "use adversary::Deserialize; #[derive(Deserialize)] struct Shadowed;",
        "use adversary::Serialize; #[derive(Serialize)] struct Shadowed;",
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
        const VERSION: &str = env!("CARGO_PKG_VERSION");
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
