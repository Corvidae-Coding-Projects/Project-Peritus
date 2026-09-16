use super::{CargoTest, Configuration, Mode, function_declarations};

#[test]
fn identifies_actual_verus_modes_and_ignores_declaration_shaped_text() {
    let source = r#"
fn ordinary() {}
const TEXT: &str = "verus! { proof fn invented() {} }";
// verus! { proof fn commented() {} }
verus! {
    pub const fn executable() -> (value: u64)
        ensures value == 1,
    {
        1
    }

    pub proof fn lemma() { assert(true); }
    pub closed spec fn model() -> bool { true }
}
"#;

    let ordinary = function_declarations(source, "ordinary");
    assert_eq!(ordinary.len(), 1);
    assert!(!ordinary[0].in_verus);
    assert_eq!(ordinary[0].mode, Some(Mode::Exec));
    assert!(function_declarations(source, "invented").is_empty());
    assert!(function_declarations(source, "commented").is_empty());

    for (name, mode) in [("executable", Mode::Exec), ("lemma", Mode::Proof), ("model", Mode::Spec)]
    {
        let declaration = function_declarations(source, name);
        assert_eq!(declaration.len(), 1);
        assert!(declaration[0].in_verus);
        assert_eq!(declaration[0].mode, Some(mode));
    }
}

#[test]
fn records_configuration_tests_nested_functions_and_ambiguous_declarations() {
    let source = r"
#[test]
fn test_case() {}

#[ignore]
#[test]
fn ignored_case() {}

#[cfg(test)]
mod cases {
    #[test]
    fn test_only_case() {}
}

verus! {
    #[cfg(any())]
    pub fn disabled() {}

    fn outer() {
        fn local() {}
    }

    pub fn duplicate() {}
    pub fn duplicate() {}
}

#[cfg(any())]
mod hidden {
    verus! { pub fn inherited_disabled() {} }
}
";

    let test = function_declarations(source, "test_case");
    assert_eq!(test.len(), 1);
    assert_eq!(test[0].cargo_test, CargoTest::Runnable);
    assert_eq!(test[0].configuration, Configuration::Unconditional);

    let ignored = function_declarations(source, "ignored_case");
    assert_eq!(ignored.len(), 1);
    assert_eq!(ignored[0].cargo_test, CargoTest::Ignored);

    let test_only = function_declarations(source, "test_only_case");
    assert_eq!(test_only.len(), 1);
    assert_eq!(test_only[0].cargo_test, CargoTest::Runnable);
    assert_eq!(test_only[0].configuration, Configuration::TestOnly);

    let disabled = function_declarations(source, "disabled");
    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].configuration, Configuration::Other);

    let inherited = function_declarations(source, "inherited_disabled");
    assert_eq!(inherited.len(), 1);
    assert_eq!(inherited[0].configuration, Configuration::Other);

    let local = function_declarations(source, "local");
    assert_eq!(local.len(), 1);
    assert!(local[0].nested);
    assert_eq!(function_declarations(source, "duplicate").len(), 2);
}
