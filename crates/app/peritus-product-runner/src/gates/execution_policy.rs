//! Authoritative task restrictions fence automatic command execution before any gate starts.

use peritus_gates::TargetGatePlan;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub(super) fn authorize(plan: &TargetGatePlan, task: &str) -> Result<(), ProductRunnerError> {
    let normalized = task.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let real_testing = real_testing_only(task);
    let real_acceptance = real_interface_acceptance(task);
    let direct_only = [
        "only by running the command",
        "only by running the program",
        "direct-execution-only",
        "direct execution only",
        "only direct execution",
        "only run the command itself",
        "no other commands",
        "no commands",
        "no shell commands",
        "do not run commands",
        "do not run shell commands",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    let prohibited = |category: &str| prohibited(&normalized, category);
    for command in plan.commands() {
        if command.program() == "peritus-internal" {
            continue;
        }
        let label = command.label().to_ascii_lowercase();
        if command.optional_when_not_selected()
            && (real_acceptance || (real_testing && label.contains("test")))
        {
            continue;
        }
        let denied = direct_only
            || (label.contains("test") && (prohibited("test") || prohibited("suite")))
            || (label.contains("lint")
                && (prohibited("lint") || prohibited_general_verification(&normalized)))
            || ((label.contains("compile") || label.contains("build"))
                && (prohibited("compile") || prohibited("compilation") || prohibited("build")));
        if denied {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Gate,
                "authorize exact-target gate commands",
                format!(
                    "Automatic check '{}' was not executed: it conflicts with the authoritative task's execution restrictions. The normal qualification policy requires this check, so Peritus cannot mark this candidate qualified. Completed edits are retained for explicit human inspection and an unqualified handoff.",
                    command.label()
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn real_testing_only(task: &str) -> bool {
    let normalized = task.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    normalized.contains("real-testing")
        && (prohibited(&normalized, "test") || prohibited(&normalized, "suite"))
}

pub(super) fn real_interface_acceptance(task: &str) -> bool {
    let normalized = task.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    normalized.contains("real-testing") && prohibited(&normalized, "qualification suite")
}

fn restricted(sentence: &str) -> bool {
    [
        "do not run",
        "do not create or run",
        "do not generate or run",
        "do not write or run",
        "don't run",
        "don't generate or run",
        "must not run",
        "without running",
    ]
    .iter()
    .any(|phrase| sentence.contains(phrase))
        || sentence.trim_start().starts_with("no ")
}

fn prohibited(normalized: &str, category: &str) -> bool {
    normalized
        .split(['.', ';', '\n'])
        .any(|sentence| restricted(sentence) && sentence.contains(category))
}

fn prohibited_general_verification(normalized: &str) -> bool {
    normalized.split(['.', ';', '\n']).any(|sentence| {
        restricted(sentence)
            && sentence.contains("verification")
            && !sentence.contains("formal verification")
    })
}
