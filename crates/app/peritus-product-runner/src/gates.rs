//! Candidate-aware D1 gate execution adapter.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use peritus_gates::{GateExecutionRecord, TargetGatePlan, TargetGateReport};

mod artifact_csv;
mod json_structure;
mod source_layout;
mod sqlite_migration;
mod yaml_structure;

use crate::{
    ProductRunnerError, ProductRunnerErrorKind, bundle::limit_text,
    developer_tools::WorkspaceOwnership, execution::ProductDeliveryScope,
};

/// Rendered exact-target gate evidence and typed D1 report.
pub struct GateReport {
    pub report: TargetGateReport,
    pub output: String,
}

pub fn run_with_ownership(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: &WorkspaceOwnership,
    delivery_scope: ProductDeliveryScope,
) -> Result<GateReport, ProductRunnerError> {
    run_scoped(root, changed_paths, Some(ownership), delivery_scope)
}

fn run_scoped(
    root: &Path,
    changed_paths: Vec<PathBuf>,
    ownership: Option<&WorkspaceOwnership>,
    delivery_scope: ProductDeliveryScope,
) -> Result<GateReport, ProductRunnerError> {
    let plan = TargetGatePlan::discover(root, changed_paths).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::Gate,
            "plan exact-target gates",
            error.to_string(),
        )
    })?;
    let mut records = Vec::new();
    for specification in plan.commands() {
        if specification.program() == "peritus-internal" {
            let record = match specification.arguments().first().map(String::as_str) {
                Some("source-layout") => source_layout::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.project().kind(),
                    specification.display(),
                    ownership,
                ),
                Some("artifact-csv-structure") => artifact_csv::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                ),
                Some("json-structure") => json_structure::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                ),
                Some("sqlite-migration") => sqlite_migration::run(
                    &root.join(specification.current_dir()),
                    specification.display(),
                ),
                Some("yaml-structure") => yaml_structure::run(
                    root,
                    specification.project().root(),
                    plan.changed_paths(),
                    specification.display(),
                ),
                _ => GateExecutionRecord {
                    command: specification.display(),
                    label: specification.label().to_owned(),
                    exit_code: None,
                    output: "unknown internal exact-target gate".to_owned(),
                },
            };
            records.push(record);
            continue;
        }
        let mut command = Command::new(specification.program());
        command.args(specification.arguments()).current_dir(root.join(specification.current_dir()));
        if specification.program() == "cargo" {
            command.env("CARGO_BUILD_JOBS", "2");
        }
        let record = match command.output() {
            Ok(output) => GateExecutionRecord {
                command: specification.display(),
                label: specification.label().to_owned(),
                exit_code: output.status.code(),
                output: limit_text(
                    &format!(
                        "{}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    ),
                    512 * 1024,
                ),
            },
            Err(error) => GateExecutionRecord {
                command: specification.display(),
                label: specification.label().to_owned(),
                exit_code: None,
                output: error.to_string(),
            },
        };
        records.push(record);
    }
    let report = TargetGateReport::from_execution(&plan, records);
    let output = render(&report, delivery_scope);
    Ok(GateReport { report, output })
}

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
fn render(report: &TargetGateReport, delivery_scope: ProductDeliveryScope) -> String {
    let mut text = String::new();
    text.push_str(&format!("Exact candidate files ({}):\n", report.changed_paths().len()));
    if report.changed_paths().is_empty() {
        if delivery_scope.allows_external_effects() {
            text.push_str(
                "  [none: caller-authorized external effects are evaluated separately]\n",
            );
        } else {
            text.push_str("  [none: acceptance is refused]\n");
        }
    } else {
        for path in report.changed_paths() {
            text.push_str(&format!("  {}\n", path.display()));
        }
    }
    if !report.uncovered_paths().is_empty() {
        text.push_str("\nUncovered candidate files:\n");
        for path in report.uncovered_paths() {
            text.push_str(&format!("  {}\n", path.display()));
        }
    }
    for record in report.records() {
        text.push_str(&format!(
            "\n[{}]\n$ {}\n{}\nexit: {}\n",
            record.label,
            record.command,
            record.output,
            record.exit_code.map_or_else(|| "not started".to_owned(), |code| code.to_string()),
        ));
    }
    let exact_target_status =
        if report.changed_paths().is_empty() && delivery_scope.allows_external_effects() {
            "NOT APPLICABLE"
        } else if report.passed() {
            "PASS"
        } else {
            "FAIL"
        };
    text.push_str(&format!("\nExact-target acceptance: {exact_target_status}\n"));
    limit_text(&text, 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn artifact_layout_checks_candidate_sources_without_rejecting_untouched_vendor_code() {
        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join("peritus-workspace.toml"),
            "schema_version = 1\nkind = \"artifact\"\n",
        )
        .expect("artifact marker");
        fs::create_dir_all(root.path().join("third_party")).expect("vendor directory");
        fs::write(root.path().join("third_party/legacy.c"), "line\n".repeat(700))
            .expect("legacy source");
        fs::write(root.path().join("vm.js"), "const ready = true;\n").expect("candidate source");

        let report = run_scoped(
            root.path(),
            vec![PathBuf::from("third_party"), PathBuf::from("vm.js")],
            None,
            ProductDeliveryScope::WorkspaceChanges,
        )
        .expect("gate report");

        assert!(report.report.passed(), "{}", report.output);
        assert!(report.output.contains("Scanned 1 changed source file"));
        assert!(!report.output.contains("legacy.c"));
    }

    #[test]
    fn nested_rust_target_cannot_be_satisfied_by_unrelated_root_tests() {
        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"root-crate\"]\nresolver = \"2\"\n",
        )
        .expect("root manifest");
        fs::create_dir_all(root.path().join("root-crate/src")).expect("root crate");
        fs::write(
            root.path().join("root-crate/Cargo.toml"),
            "[package]\nname = \"root-crate\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("manifest");
        fs::write(root.path().join("root-crate/src/lib.rs"), "pub fn ok() -> bool { true }\n")
            .expect("source");
        fs::create_dir_all(root.path().join("game/src")).expect("game");
        fs::write(
            root.path().join("game/Cargo.toml"),
            "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("game manifest");
        fs::write(root.path().join("game/src/main.rs"), "fn main() { assert!(true); }\n")
            .expect("game source");

        let report = run_scoped(
            root.path(),
            vec![PathBuf::from("game/Cargo.toml"), PathBuf::from("game/src/main.rs")],
            None,
            ProductDeliveryScope::WorkspaceChanges,
        )
        .expect("gate report");
        let nested_manifest =
            PathBuf::from("game").join("Cargo.toml").to_string_lossy().into_owned();
        let root_manifest =
            PathBuf::from("root-crate").join("Cargo.toml").to_string_lossy().into_owned();

        assert!(!report.report.passed());
        assert!(report.output.contains("--manifest-path"));
        assert!(report.output.contains(&nested_manifest));
        assert!(!report.output.contains(&root_manifest));
    }
}
