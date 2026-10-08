//! Positive and hostile regressions for the explicitly modeled persistence/expression forms.

use super::scanner::scan;
use super::violation::ViolationKind;

#[test]
fn serde_metadata_preserves_closed_import_and_callback_rules() {
    let accepted = scan(
        r#"
        use serde::{Deserialize, Serialize};
        #[derive(Serialize, Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Entry { Live { #[serde(rename = "max_requests")] requests: u32 } }
        #[derive(Serialize, Deserialize)]
        struct State {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            optional: Option<u8>,
            #[serde(default, skip_serializing_if = "super::TaskBrief::is_empty")]
            brief: TaskBrief,
            #[serde(default, skip_serializing_if = "String::is_empty")]
            stderr: String,
            #[serde(default, skip_serializing_if = "std::ops::Not::not")]
            truncated: bool,
        }
    "#,
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);
    for metadata in [
        "flatten",
        "skip",
        "default = \"unsafe_default\"",
        "try_from = \"Unchecked\"",
        "deserialize_with = \"unchecked\"",
        "rename_all = \"SCREAMING_SNAKE_CASE\"",
        "tag = \"\"",
        "tag = \"kind\", tag = \"other\"",
        "default, unknown",
        "default, skip_serializing_if = \"adversary::hide\"",
        "default, skip_serializing_if = \"Option::is_none()\"",
        "default, skip_serializing_if = \"adversary::Not::not\"",
        "default, skip_serializing_if = \"std::ops::Not::not()\"",
        "default, skip_serializing_if = \"Option::is_none\", flatten",
    ] {
        let source = format!(
            "use serde::Deserialize; use serde::Serialize; #[serde({metadata})] struct Bad;"
        );
        assert!(!scan(&source).violations.is_empty(), "{source}");
    }
    for source in [
        "use serde::{Deserialize, adversary::Serialize}; #[derive(Serialize)] struct Bad;",
        "use serde::{Deserialize, Other as Serialize}; #[derive(Serialize)] struct Bad;",
        "use serde::Deserialize; #[serde(skip_serializing_if = \"Option::is_none\")] struct Bad;",
    ] {
        assert!(!scan(source).violations.is_empty(), "{source}");
    }
}

#[test]
fn serialize_only_serde_metadata_preserves_its_direction_boundary() {
    let accepted = scan(
        r#"
        use serde::Serialize;
        #[derive(Serialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum Checkpoint {
            Complete { #[serde(rename = "run_id")] run: String },
        }
        #[derive(Serialize)]
        struct Journal {
            #[serde(skip_serializing_if = "Option::is_none")]
            cursor: Option<u64>,
        }
    "#,
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);

    for metadata in [
        "default",
        "deny_unknown_fields",
        "rename_all = \"snake_case\", default",
        "serialize_with = \"unchecked\"",
        "deserialize_with = \"unchecked\"",
        "try_from = \"Unchecked\"",
    ] {
        let source =
            format!("use serde::Serialize; #[derive(Serialize)] #[serde({metadata})] struct Bad;");
        assert!(
            scan(&source)
                .violations
                .iter()
                .any(|violation| violation.kind == ViolationKind::UnsupportedAttribute),
            "{source}"
        );
    }
    assert!(
        scan("#[serde(rename_all = \"snake_case\")] struct Bad;")
            .violations
            .iter()
            .any(|violation| violation.kind == ViolationKind::UnsupportedAttribute)
    );
}

#[test]
fn audited_expression_macros_keep_payload_contracts_visible() {
    let accepted = scan(
        r#"
        use serde_json::json;
        fn expressions() {
            let _ = json!({"key": [1, serde_json::json!({"nested":true})]});
            tokio::pin!(operation);
            tokio::select! { biased; result = &mut operation => return result, () = wait() => {} }
            println!("observation");
            eprintln!("diagnostic");
            let _ = rusqlite::params![1_u8];
            writeln!(output, "observation");
            unreachable!("exhaustive state");
        }
        #[tokio::test]
        async fn asynchronous_test() { let _ = 1_u8; }
        #[test]
        #[ignore = "requires a native display server"]
        fn native_test() { let _ = 1_u8; }
        #[derive(Default)]
        enum Mode { #[default] First, Second }
    "#,
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);
    for source in [
        "serde_json::json!({ pub fn hidden() requires false {} 1 });",
        "rusqlite::params![{ pub fn hidden() requires false {} 1 }];",
        "eprintln!({ pub fn hidden() requires false {} });",
        "tokio::select! { _ = f() => { pub fn hidden() requires false {} } }",
        "#[tokio::test] pub async fn hidden() requires false {}",
        "#[ignore = \"native\"] pub fn hidden() requires false {}",
    ] {
        assert!(
            scan(source).violations.iter().any(|v| v.kind == ViolationKind::ExposedRequires),
            "{source}"
        );
    }
    assert!(
        scan("serde_json::json!({ evil!() });")
            .violations
            .iter()
            .any(|v| v.kind == ViolationKind::UnsupportedMacro)
    );
}

#[test]
fn rejected_macro_namespaces_cannot_impersonate_reviewed_dependencies() {
    for source in [
        "json!({});",
        "use adversary::json; json!({});",
        "use adversary::evil as json; json!({});",
        "use serde_json::json as renamed; renamed!({});",
        "use serde_json::*; json!({});",
        "mod serde_json {} serde_json::json!({});",
        "mod rusqlite {} rusqlite::params![1];",
        "use adversary as rusqlite; rusqlite::params![1];",
        "crate::rusqlite::params![1];",
        "::rusqlite::params![1];",
        "rusqlite::r#params![1];",
        "use adversary::eprintln; eprintln!(\"hidden\");",
        "mod tokio {} tokio::select! { _ = f() => {} }",
        "use adversary as tokio; #[tokio::test] async fn hidden() { let _ = 1_u8; }",
        "extern crate adversary as tokio; tokio::pin!(operation);",
        "use crate::adversary as serde_json; serde_json::json!({});",
        "crate::serde_json::json!({});",
        "adversary::select!{}",
        "::tokio::pin!(operation);",
        "tokio::r#pin!(operation);",
        "tokio::pin!(let operation = f(););",
        "#[tokio::test(flavor = \"multi_thread\")] async fn unknown_configuration() { let _ = 1_u8; }",
        "#[ignore] fn no_reason() { let _ = 1_u8; }",
        "#[ignore = \"\"] fn no_reason() { let _ = 1_u8; }",
        "#[default(arbitrary)] enum Bad {}",
    ] {
        assert!(!scan(source).violations.is_empty(), "{source}");
    }
}

#[test]
fn permits_only_private_const_standard_thread_locals() {
    let accepted = scan(
        r"
        use std::cell::{Cell, RefCell};
        std::thread_local! {
            static COUNT: Cell<u64> = const { Cell::new(0) };
            static VALUES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        ",
    );
    assert!(accepted.violations.is_empty(), "{:?}", accepted.violations);

    let nested_contract = scan(
        r"
        std::thread_local! {
            static VALUE: u8 = const {
                pub fn hidden() requires false { }
                0
            };
        }
        ",
    );
    assert!(
        nested_contract
            .violations
            .iter()
            .any(|violation| violation.kind == ViolationKind::ExposedRequires),
        "{:?}",
        nested_contract.violations
    );

    let nested_macro = scan(
        r"
        std::thread_local! {
            static VALUE: u8 = const { evil!(); 0 };
        }
        ",
    );
    assert!(
        nested_macro
            .violations
            .iter()
            .any(|violation| violation.kind == ViolationKind::UnsupportedMacro),
        "{:?}",
        nested_macro.violations
    );
}

#[test]
fn rejects_shadowed_aliased_and_nested_thread_local_namespaces() {
    for source in [
        "mod std {} std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "use adversary as std; std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "use adversary::std; std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "pub use adversary as std; std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "extern crate adversary as std; std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "crate::std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "::std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "outer::std::thread_local! { static VALUE: u8 = const { 0 }; }",
        "adversary::thread_local! { static VALUE: u8 = const { 0 }; }",
        "use std::thread_local; thread_local! { static VALUE: u8 = const { 0 }; }",
        "std::r#thread_local! { static VALUE: u8 = const { 0 }; }",
    ] {
        assert!(
            scan(source)
                .violations
                .iter()
                .any(|violation| violation.kind == ViolationKind::UnsupportedMacro),
            "{source}"
        );
    }
}

#[test]
fn rejects_thread_local_declarations_outside_the_closed_shape() {
    for source in [
        "std::thread_local! {}",
        "std::thread_local! { pub static VALUE: u8 = const { 0 }; }",
        "std::thread_local! { pub(crate) static VALUE: u8 = const { 0 }; }",
        "std::thread_local! { #[doc = \"public surface\"] static VALUE: u8 = const { 0 }; }",
        "std::thread_local! { static VALUE: u8 = 0; }",
        "std::thread_local! { static VALUE: u8 = const { 0 } }",
        "std::thread_local! { static VALUE = const { 0 }; }",
        "std::thread_local! { static VALUE: = const { 0 }; }",
        "std::thread_local! { static VALUE: u8 = const { 0 };; }",
        "std::thread_local! { static VALUE: u8 = const { 0 }; trailing }",
        "std::thread_local!(static VALUE: u8 = const { 0 };);",
    ] {
        assert!(
            scan(source)
                .violations
                .iter()
                .any(|violation| violation.kind == ViolationKind::UnsupportedMacro),
            "{source}"
        );
    }
}
