//! Regression for an explicitly requested design update inside a Cargo workspace.

use super::*;

fn workspace() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = 'document-check'\nversion = '0.1.0'\nedition = '2024'\n",
    )
    .unwrap();
    std::fs::write(root.path().join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n").unwrap();
    std::fs::write(root.path().join("design.md"), "# Design\n\nPreserve the existing behavior.\n")
        .unwrap();
    root
}

#[test]
fn requested_design_output_in_cargo_workspace_does_not_schedule_cargo() {
    let root = workspace();
    let paths = vec![PathBuf::from("design.md")];
    let plan = TargetGatePlan::discover(root.path(), paths.clone(), &paths).unwrap();
    assert!(plan.has_complete_coverage());
    assert!(
        plan.commands().iter().all(|command| command.program() == "peritus-internal"),
        "document artifact verification must not compile the enclosing software project"
    );
    assert!(plan.commands().iter().any(|command| command.label() == "Source readability"));
    let task = "Update design.md only. Do not run Cargo builds or tests.";
    let report = run_scoped_with_cancellation(
        root.path(),
        paths,
        None,
        ProductDeliveryScope::WorkspaceChanges,
        task,
        task,
        &GateCancellation::new(
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            peritus_provider_core::CancellationToken::new(),
        ),
    )
    .unwrap();
    assert!(report.report.passed(), "{}", report.output);
    assert!(!report.output.contains("cargo build"));
}

#[test]
fn source_changes_keep_cargo_checks_even_with_an_explicit_document_output() {
    let root = workspace();
    let paths = vec![PathBuf::from("design.md"), PathBuf::from("src/lib.rs")];
    let plan = TargetGatePlan::discover(root.path(), paths.clone(), &paths).unwrap();
    assert!(plan.has_complete_coverage());
    assert!(plan.commands().iter().any(|command| command.program() == "cargo"));
    assert!(execution_policy::authorize(&plan, "Do not run Cargo builds or tests.").is_err());
}

#[test]
fn an_unrequested_markdown_change_keeps_the_existing_project_contract() {
    let root = workspace();
    let plan =
        TargetGatePlan::discover(root.path(), vec![PathBuf::from("design.md")], &[]).unwrap();
    assert!(plan.commands().iter().any(|command| command.program() == "cargo"));
}
