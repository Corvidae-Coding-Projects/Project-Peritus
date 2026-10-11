//! Authoritative task restrictions fence automatic command execution before any gate starts.

use peritus_gates::TargetGatePlan;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

pub(super) fn authorize(plan: &TargetGatePlan, task: &str) -> Result<(), ProductRunnerError> {
    let normalized = task.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
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
    let prohibited = |category: &str| {
        normalized.split(['.', ';', '\n']).any(|sentence| {
            let sentence = sentence.trim();
            let restriction = sentence.contains("do not run")
                || sentence.contains("do not create or run")
                || sentence.contains("don't run")
                || sentence.contains("must not run")
                || sentence.contains("without running")
                || sentence.starts_with("no ");
            restriction && sentence.contains(category)
        })
    };
    for command in plan.commands() {
        if command.program() == "peritus-internal" {
            continue;
        }
        let label = command.label().to_ascii_lowercase();
        let denied = direct_only
            || (label.contains("test") && (prohibited("test") || prohibited("suite")))
            || (label.contains("lint") && (prohibited("lint") || prohibited("verification")))
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
