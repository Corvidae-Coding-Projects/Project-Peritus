use super::*;

mod manifestless;

#[test]
fn rust_plan_builds_the_exact_nested_package() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    let project = temporary.path().join("game");
    std::fs::create_dir_all(project.join("src")).expect("nested package directory");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"game\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .expect("nested package manifest");
    std::fs::write(project.join("src/main.rs"), "fn main() { println!(\"game\"); }\n")
        .expect("nested package source");

    let plan =
        TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("game/src/main.rs")], &[])
            .expect("exact target plan");
    let manifest = PathBuf::from("game").join("Cargo.toml").to_string_lossy().into_owned();

    let build = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Rust build")
        .expect("Rust build gate");
    let format = plan
        .commands()
        .iter()
        .find(|command| command.label() == "Rust format")
        .expect("Rust format gate");
    assert_eq!(format.program(), "cargo");
    assert_eq!(format.current_dir(), Path::new(""));
    assert_eq!(
        format.arguments(),
        ["fmt", "--manifest-path", manifest.as_str(), "--all", "--", "--check"]
    );
    assert_eq!(build.program(), "cargo");
    assert_eq!(build.current_dir(), Path::new(""));
    assert_eq!(
        build.arguments(),
        [
            "build",
            "--locked",
            "--all-targets",
            "--all-features",
            "--manifest-path",
            manifest.as_str(),
            "--workspace",
        ]
    );
}

#[test]
fn explicit_artifact_workspace_covers_general_outputs() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    std::fs::create_dir(temporary.path().join("out")).expect("output directory");
    std::fs::write(temporary.path().join("out/result.txt"), "result\n").expect("output artifact");

    let plan =
        TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("out/result.txt")], &[])
            .expect("artifact plan");

    assert!(plan.has_complete_coverage());
    assert!(plan.uncovered_paths().is_empty());
    assert_eq!(plan.projects()[0].kind(), ProjectKind::Artifact);
    assert_eq!(plan.commands().len(), 2);
    assert_eq!(plan.commands()[0].label(), "Source layout");
    assert_eq!(plan.commands()[1].label(), "Artifact CSV structure");
}

#[test]
fn exact_requested_artifact_does_not_cover_an_unrequested_sibling() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(temporary.path().join("result.txt"), "result").expect("requested artifact");
    std::fs::write(temporary.path().join("notes.txt"), "notes").expect("unrequested artifact");

    let requested = [PathBuf::from("result.txt")];
    let exact =
        TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("result.txt")], &requested)
            .expect("requested artifact plan");

    assert!(exact.has_complete_coverage());
    assert!(exact.uncovered_paths().is_empty());
    assert_eq!(exact.projects()[0].kind(), ProjectKind::Artifact);
    assert_eq!(exact.projects()[0].manifest(), None);
    assert_eq!(exact.commands().len(), 1);
    assert_eq!(exact.commands()[0].label(), "Source layout");

    let with_sibling = TargetGatePlan::discover(
        temporary.path(),
        vec![PathBuf::from("result.txt"), PathBuf::from("notes.txt")],
        &requested,
    )
    .expect("mixed artifact plan");

    assert!(!with_sibling.has_complete_coverage());
    assert_eq!(with_sibling.uncovered_paths(), [PathBuf::from("notes.txt")]);
}

#[test]
fn changed_json_artifact_gets_structural_acceptance() {
    let temporary = tempfile::tempdir().expect("temporary workspace");
    std::fs::write(
        temporary.path().join("peritus-workspace.toml"),
        "schema_version = 1\nkind = \"artifact\"\n",
    )
    .expect("artifact workspace marker");
    std::fs::create_dir(temporary.path().join("out")).expect("output directory");
    std::fs::write(temporary.path().join("out/result.json"), "{\"ok\":true}\n")
        .expect("JSON artifact");

    let plan =
        TargetGatePlan::discover(temporary.path(), vec![PathBuf::from("out/result.json")], &[])
            .expect("artifact plan");

    assert!(plan.has_complete_coverage());
    assert!(plan.commands().iter().any(|command| command.label() == "JSON structure"));
}
