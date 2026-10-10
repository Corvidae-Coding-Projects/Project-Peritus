//! Deterministic reconciliation of closed deliverable requests with final changed paths.

use std::path::PathBuf;

use peritus_gates::GateExecutionRecord;

use super::explicit_paths;

pub(super) fn run(
    analysis: &explicit_paths::Analysis,
    changed_paths: &[PathBuf],
) -> GateExecutionRecord {
    let required_outputs = analysis.required_outputs();
    let closed_target = analysis
        .requests_single_file()
        .then_some(required_outputs.as_slice())
        .and_then(|paths| match paths {
            [path] => Some(path),
            _ => None,
        });

    let Some(target) = closed_target else {
        return record(
            0,
            "No explicit single-file deliverable requires a closed inventory check.\n",
        );
    };

    let unexpected =
        changed_paths.iter().filter(|path| path.as_path() != target).collect::<Vec<_>>();
    let target_is_changed = changed_paths.iter().any(|path| path == target);
    if target_is_changed && unexpected.is_empty() {
        return record(
            0,
            &format!(
                "Closed single-file deliverable: {}\nFinal changed-path inventory: PASS\n",
                target.display(),
            ),
        );
    }

    let mut output = format!(
        "Closed single-file deliverable: {}\nFinal changed-path inventory failures:\n",
        target.display(),
    );
    if !target_is_changed {
        output.push_str("  - requested deliverable is not present in the changed-path inventory\n");
    }
    for path in unexpected {
        output.push_str("  - unexpected changed path outside the single-file deliverable: ");
        output.push_str(&path.display().to_string());
        output.push('\n');
    }
    record(1, &output)
}

fn record(exit_code: i32, output: &str) -> GateExecutionRecord {
    GateExecutionRecord {
        command: "peritus-internal deliverable-inventory".to_owned(),
        label: "Closed deliverable inventory".to_owned(),
        exit_code: Some(exit_code),
        output: output.to_owned(),
    }
}
