use super::{check, parse};
use crate::api_contract::Mode;
use crate::metadata;
use crate::model::ToolchainPolicy;
use crate::trust::RegisteredProofSymbol;
use serde_json::{Value, json};
use std::path::Path;

fn tools() -> ToolchainPolicy {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace");
    metadata::toolchain_policy(root).expect("pinned tools")
}

fn compiler() -> Value {
    let tools = tools();
    json!({
        "func-details": {
            "peritus_example::implementation": {},
            "peritus_example::specification": {},
            "vstd::unrelated": {}
        },
        "verification-results": {
            "encountered-error": false, "encountered-vir-error": false,
            "success": true, "errors": 0, "is-verifying-entire-crate": true
        },
        "verus": {
            "version": tools.verus, "commit": tools.vstd_revision,
            "toolchain": format!("{}-x86_64-unknown-linux-gnu", tools.rust)
        },
        "times-ms": { "smt": { "smt-run-module-times": [{
            "function-breakdown": [{
                "function": "peritus_example::implementation", "mode:": "exec", "success": true
            }]
        }] } }
    })
}

fn expected() -> Vec<RegisteredProofSymbol> {
    vec![RegisteredProofSymbol {
        obligation: "INV-EXAMPLE".into(),
        owner: "peritus-example".into(),
        symbol: "peritus_example::specification".into(),
        mode: Mode::Spec,
    }]
}

fn accepts(value: &Value) -> bool {
    let bytes = serde_json::to_vec(value).expect("fixture JSON");
    parse(&bytes)
        .and_then(|reports| {
            check(&reports, "peritus-example", &["peritus_example"], &expected(), &tools())
        })
        .is_ok()
}

#[test]
fn separates_selected_specifications_from_actual_executable_queries() {
    let bytes = serde_json::to_vec(&compiler()).expect("fixture JSON");
    let reports = parse(&bytes).expect("compiler report");
    let scope = check(&reports, "peritus-example", &["peritus_example"], &expected(), &tools())
        .expect("valid selection");
    assert_eq!(scope.registered_symbols, 1);
    assert_eq!(scope.queried_functions.len(), 1);
    assert_eq!(scope.selected_functions.len(), 2);
    assert_eq!(scope.queried_functions["peritus_example::implementation"], "exec");
    assert!(scope.has_solver_queries);
}

#[test]
fn decodes_concatenated_reports_without_counting_imports_as_new_proofs() {
    let text =
        format!("verification results:: 24 verified, 0 errors\n{}\n{}\n", compiler(), compiler());
    let reports = parse(text.as_bytes()).expect("two reports");
    assert_eq!(reports.len(), 2);
    let scope = check(&reports, "peritus-example", &["peritus_example"], &expected(), &tools())
        .expect("scope");
    assert_eq!(scope.queried_functions.len(), 1);
}

#[test]
fn rejects_successful_compilation_that_did_not_select_registered_evidence() {
    let mut value = compiler();
    value["func-details"]
        .as_object_mut()
        .expect("functions")
        .remove("peritus_example::specification");
    assert!(!accepts(&value));
}

#[test]
fn executable_and_proof_evidence_require_successful_queries_in_the_declared_mode() {
    for (mode, query_mode) in [(Mode::Exec, "exec"), (Mode::Proof, "proof")] {
        let mut evidence = expected();
        evidence[0].symbol = "peritus_example::implementation".into();
        evidence[0].mode = mode;
        let reports = parse(compiler().to_string().as_bytes()).expect("compiler report");
        assert_eq!(
            check(&reports, "peritus-example", &["peritus_example"], &evidence, &tools()).is_ok(),
            query_mode == "exec"
        );

        let mut value = compiler();
        value["times-ms"]["smt"]["smt-run-module-times"][0]["function-breakdown"][0]["mode:"] =
            json!(query_mode);
        let reports = parse(value.to_string().as_bytes()).expect("compiler report");
        assert!(
            check(&reports, "peritus-example", &["peritus_example"], &evidence, &tools()).is_ok()
        );
    }

    let mut value = compiler();
    value["times-ms"]["smt"]["smt-run-module-times"] = json!([]);
    let reports = parse(value.to_string().as_bytes()).expect("selected-only compiler report");
    let mut evidence = expected();
    evidence[0].symbol = "peritus_example::implementation".into();
    evidence[0].mode = Mode::Exec;
    assert!(check(&reports, "peritus-example", &["peritus_example"], &evidence, &tools()).is_err());
}

#[test]
fn rejects_failed_partial_and_missing_compiler_results() {
    for field in ["encountered-error", "encountered-vir-error"] {
        let mut value = compiler();
        value["verification-results"][field] = json!(true);
        assert!(!accepts(&value));
    }
    for field in ["success", "is-verifying-entire-crate"] {
        let mut value = compiler();
        value["verification-results"][field] = json!(false);
        assert!(!accepts(&value));
    }
    let mut value = compiler();
    value["verification-results"]["errors"] = json!(1);
    assert!(!accepts(&value));
    value.as_object_mut().expect("object").remove("times-ms");
    assert!(!accepts(&value));
    assert!(parse(b"verification results:: 0 verified, 0 errors\n").is_err());
}

#[test]
fn rejects_other_toolchains_and_conflicting_function_modes() {
    for field in ["version", "commit", "toolchain"] {
        let mut value = compiler();
        value["verus"][field] = json!("unreviewed");
        assert!(!accepts(&value));
    }
    let mut second = compiler();
    second["times-ms"]["smt"]["smt-run-module-times"][0]["function-breakdown"][0]["mode:"] =
        json!("proof");
    let text = format!("{}\n{second}", compiler());
    let reports = parse(text.as_bytes()).expect("two valid objects");
    assert!(
        check(&reports, "peritus-example", &["peritus_example"], &expected(), &tools()).is_err()
    );
}

#[test]
fn distinguishes_a_fresh_zero_query_report_from_a_missing_package_report() {
    let mut empty = compiler();
    empty["func-details"] = json!({});
    empty["times-ms"]["smt"]["smt-run-module-times"] = json!([]);
    let reports = parse(empty.to_string().as_bytes()).expect("fresh empty root");
    let scope =
        check(&reports, "peritus-empty", &["peritus_empty"], &[], &tools()).expect("empty scope");
    assert!(!scope.has_solver_queries);
    assert!(scope.selected_functions.is_empty());
    assert!(check(&[], "peritus-empty", &["peritus_empty"], &[], &tools()).is_err());
    assert!(check(&reports, "peritus-empty", &[], &[], &tools()).is_err());
}

#[test]
fn imported_symbols_from_a_sibling_root_cannot_replace_its_invocation() {
    let reports = parse(compiler().to_string().as_bytes()).expect("different root");
    assert!(check(&reports, "peritus-empty", &["peritus_empty"], &[], &tools()).is_err());
}

#[test]
fn accepts_actual_library_and_codegen_root_names_from_one_package() {
    let text = compiler().to_string().replace("peritus_example", "peritus_example_codegen");
    let reports = parse(text.as_bytes()).expect("codegen root");
    let scope = check(
        &reports,
        "peritus-example",
        &["peritus_example", "peritus-example-codegen"],
        &[],
        &tools(),
    )
    .expect("registered codegen target");
    assert!(scope.has_solver_queries);
}

#[test]
fn rejects_unknown_text_truncation_and_dependency_failures() {
    for text in ["unrecognized output", "{", "verification results:: 2 verified, 1 errors"] {
        assert!(parse(text.as_bytes()).is_err());
    }
    let text = format!("{}\n{{", compiler());
    assert!(parse(text.as_bytes()).is_err());
}

#[test]
fn method_evidence_requires_the_exact_defining_owner_in_selection_and_queries() {
    let canonical = "peritus_example::accounting::AccountingState::apply_usage";
    let evidence = vec![RegisteredProofSymbol {
        obligation: "OBL-METHOD".into(),
        owner: "peritus-example".into(),
        symbol: canonical.into(),
        mode: Mode::Exec,
    }];
    for selected in [
        canonical,
        "peritus_example::accounting::usage::AccountingState::apply_usage",
        "peritus_example::accounting::OtherState::apply_usage",
        "peritus_example::accounting::State::apply_usage",
    ] {
        for queried in [canonical, selected] {
            let mut value = compiler();
            value["func-details"] = json!({selected: {}});
            value["times-ms"]["smt"]["smt-run-module-times"][0]["function-breakdown"] =
                json!([{"function": queried, "mode:": "exec", "success": true}]);
            let reports = parse(value.to_string().as_bytes()).expect("compiler report");
            assert_eq!(
                check(&reports, "peritus-example", &["peritus_example"], &evidence, &tools())
                    .is_ok(),
                selected == canonical && queried == canonical,
                "selected {selected}, queried {queried}",
            );
        }
    }
}
