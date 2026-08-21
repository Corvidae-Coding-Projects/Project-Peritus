use super::reproducibility_workflow_tests::{assert_message, validate};
use super::workflow_files::DocumentKind;

#[test]
fn workflow_local_script_cannot_hide_unlocked_cargo() {
    let yaml = r"
name: script bypass
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: ./ci.sh
";
    let (_, diagnostics) = validate(".github/workflows/extra.yml", DocumentKind::Workflow, yaml);
    assert_message(&diagnostics, "unaudited executable");
}

#[test]
fn composite_action_local_script_cannot_hide_unlocked_cargo() {
    let yaml = r"
name: script bypass
runs:
  using: composite
  steps:
    - shell: bash
      run: scripts/verify.sh
";
    let (_, diagnostics) =
        validate(".github/actions/bypass/action.yml", DocumentKind::Action, yaml);
    assert_message(&diagnostics, "unaudited executable");
}

#[test]
fn workflow_defaults_cannot_replace_the_checked_interpreter() {
    let yaml = r"
name: shell bypass
defaults:
  run:
    shell: ./ci.sh {0}
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test --workspace --locked
";
    let (_, diagnostics) = validate(".github/workflows/extra.yml", DocumentKind::Workflow, yaml);
    assert_message(&diagnostics, "can replace the inspected run-command interpreter");
}

#[test]
fn composite_action_shell_template_must_be_exact_bash() {
    let yaml = r"
name: shell bypass
runs:
  using: composite
  steps:
    - shell: ./ci.sh {0}
      run: cargo test --workspace --locked
";
    let (_, diagnostics) =
        validate(".github/actions/bypass/action.yml", DocumentKind::Action, yaml);
    assert_message(&diagnostics, "not the exact reviewed");
}
