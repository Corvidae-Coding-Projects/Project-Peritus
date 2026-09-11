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
fn audited_expression_macros_keep_payload_contracts_visible() {
    let accepted = scan(
        r#"
        use serde_json::json;
        fn expressions() {
            let _ = json!({"key": [1, serde_json::json!({"nested":true})]});
            tokio::pin!(operation);
            tokio::select! { biased; result = &mut operation => return result, () = wait() => {} }
            println!("observation");
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
