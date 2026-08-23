use super::scanner::scan;
use super::violation::ViolationKind;

#[test]
fn relational_contract_operators_do_not_hide_later_preconditions() {
    let result = scan(
        r"
        pub fn comparison(left: u64, right: u64) -> (valid: bool)
            ensures valid == (left < right && right > 0)
        { left < right }

        pub fn braced_contract(value: u64) -> (valid: bool)
            ensures match valid { true => true, false => value == 0 },
            requires value > 0,
        { true }
        ",
    );

    assert_eq!(result.executable_entry_points, 2);
    assert_eq!(
        result
            .violations
            .iter()
            .filter(|violation| violation.kind == ViolationKind::ExposedRequires)
            .count(),
        1
    );
    assert!(
        result
            .violations
            .iter()
            .all(|violation| violation.kind != ViolationKind::UnparseableHeader)
    );
}

#[test]
fn permits_audited_standard_and_rusqlite_expansions() {
    let result = scan(
        r#"
        use rusqlite::{Connection, params};

        #[derive(Clone, Default)]
        #[non_exhaustive]
        pub struct State;

        pub fn bind(connection: &Connection) {
            let _ = connection.execute("SELECT ?1", params![1_u8]);
        }

        pub fn render(formatter: &mut Formatter<'_>) -> Result<(), Error> {
            write!(formatter, "state")
        }
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
fn rejects_unproven_dependency_macro_bindings_and_standard_macro_shadowing() {
    for source in [
        "use adversary::params; fn query() { let _ = params![1_u8]; }",
        "use rusqlite::{nested::params}; fn query() { let _ = params![1_u8]; }",
        "use rusqlite::params as values; fn query() { let _ = values![1_u8]; }",
        "use crate::shadow::write; fn render() { write!(sink, \"value\"); }",
        "fn raw_keyword() { r#if! { pub fn generated() { let _ = 1_u8; } } }",
    ] {
        let result = scan(source);
        assert!(
            result
                .violations
                .iter()
                .any(|violation| violation.kind == ViolationKind::UnsupportedMacro),
            "{source}: {:?}",
            result.violations
        );
    }
}

#[test]
fn unary_not_after_a_keyword_is_not_a_macro_invocation() {
    let result = scan(
        r"
        pub fn select(flag: bool) -> bool {
            if !(flag) { return !(flag); }
            while !(flag) { break; }
            flag
        }
        ",
    );

    assert!(
        result.violations.iter().all(|violation| violation.kind != ViolationKind::UnsupportedMacro),
        "{:?}",
        result.violations
    );
}
