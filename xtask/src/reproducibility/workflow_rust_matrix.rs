//! Shared validation for exceptional reviewed Rust matrix entries.

use yaml_rust2::Yaml;

const PLATFORM_TERMINAL_ENTRIES: [(&str, &str); 9] = [
    ("ubuntu-24.04", "test-platform-terminal-interactive"),
    ("ubuntu-24.04", "test-platform-terminal-signal"),
    ("ubuntu-24.04", "test-platform-terminal-cancel"),
    ("macos-15", "test-platform-terminal-interactive"),
    ("macos-15", "test-platform-terminal-signal"),
    ("macos-15", "test-platform-terminal-cancel"),
    ("windows-2025", "test-platform-terminal-interactive"),
    ("windows-2025", "test-platform-terminal-signal"),
    ("windows-2025", "test-platform-terminal-cancel"),
];

const RUNNER_ENTRIES: [(&str, &str); 6] = [
    ("ubuntu-24.04", "test-runner-recovery"),
    ("ubuntu-24.04", "test-runner-product"),
    ("macos-15", "test-runner-recovery"),
    ("macos-15", "test-runner-product"),
    ("windows-2025", "test-runner-recovery"),
    ("windows-2025", "test-runner-product"),
];

const DAEMON_ENTRIES: [(&str, &str); 3] =
    [("ubuntu-24.04", "test-daemon"), ("macos-15", "test-daemon"), ("windows-2025", "test-daemon")];

pub(super) fn has_exact_test_includes(value: Option<&Yaml>) -> bool {
    let Some(entries) = value.and_then(Yaml::as_vec) else { return false };
    let expected = PLATFORM_TERMINAL_ENTRIES
        .into_iter()
        .map(|(os, operation)| (os, operation, "testing-platform"))
        .chain(RUNNER_ENTRIES.into_iter().map(|(os, operation)| (os, operation, "app-runner")))
        .chain(DAEMON_ENTRIES.into_iter().map(|(os, operation)| (os, operation, "app-shell")));
    entries.len() == PLATFORM_TERMINAL_ENTRIES.len() + RUNNER_ENTRIES.len() + DAEMON_ENTRIES.len()
        && entries.iter().zip(expected).all(|(entry, expected)| {
            let Some(entry) = entry.as_hash() else { return false };
            entry.len() == 3
                && string(entry, "os") == Some(expected.0)
                && string(entry, "operation") == Some(expected.1)
                && string(entry, "shard") == Some(expected.2)
        })
}

fn string<'a>(mapping: &'a yaml_rust2::yaml::Hash, key: &str) -> Option<&'a str> {
    mapping.get(&Yaml::String(key.to_owned())).and_then(Yaml::as_str)
}

#[cfg(test)]
mod tests {
    use super::{DAEMON_ENTRIES, RUNNER_ENTRIES};
    use crate::reproducibility::reproducibility_workflow_fixture::{
        canonical_ci, canonical_governance,
    };
    use crate::reproducibility::reproducibility_workflow_tests::{assert_message, validate};
    use crate::reproducibility::workflow_files::DocumentKind;

    #[test]
    fn both_gates_require_each_native_runner_and_daemon_partition_exactly_once() {
        for (path, canonical, message) in [
            (".github/workflows/ci.yml", canonical_ci(), "exact unconditional Rust shard matrix"),
            (
                ".github/workflows/formal-governance.yml",
                canonical_governance(),
                "does not retain every hardcoded job and final status",
            ),
        ] {
            let entries = RUNNER_ENTRIES
                .into_iter()
                .map(|(os, operation)| (os, operation, "app-runner"))
                .chain(
                    DAEMON_ENTRIES.into_iter().map(|(os, operation)| (os, operation, "app-shell")),
                );
            for (os, operation, shard) in entries {
                let entry =
                    format!("          - {{ os: {os}, operation: {operation}, shard: {shard} }}\n");
                let wrong_shard = if shard == "app-shell" { "app-runner" } else { "app-shell" };
                for altered in [
                    canonical.replace(&entry, ""),
                    canonical.replace(&entry, &format!("{entry}{entry}")),
                    canonical.replace(
                        &entry,
                        &entry
                            .replace(&format!("shard: {shard}"), &format!("shard: {wrong_shard}")),
                    ),
                ] {
                    assert_ne!(altered, canonical, "fixture mutation must change the workflow");
                    let (_, diagnostics) = validate(path, DocumentKind::Workflow, &altered);
                    assert_message(&diagnostics, message);
                }
            }
        }
    }
}
