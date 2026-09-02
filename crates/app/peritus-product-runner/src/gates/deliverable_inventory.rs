//! Deterministic reconciliation of closed deliverable requests with final changed paths.

use std::path::{Path, PathBuf};

use peritus_gates::GateExecutionRecord;

use super::explicit_paths;

pub(super) fn run(root: &Path, transcript: &str, changed_paths: &[PathBuf]) -> GateExecutionRecord {
    let required_outputs = explicit_paths::required_outputs(root, transcript);
    let closed_target = explicit_paths::requests_single_file(transcript)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_compiler_byproduct_beside_requested_single_file() {
        let root = tempfile::tempdir().expect("root");
        let transcript = format!(
            "Write me a single file in {0}/polyglot/main.py.c which is a polyglot.\n\
             Run gcc {0}/polyglot/main.py.c -o {0}/polyglot/cmain for verification.",
            root.path().display(),
        );

        let result = run(
            root.path(),
            &transcript,
            &[PathBuf::from("polyglot/main.py.c"), PathBuf::from("polyglot/cmain")],
        );

        assert_eq!(result.exit_code, Some(1));
        assert!(result.output.contains("unexpected changed path"));
        assert!(result.output.contains("polyglot/cmain"));
    }

    #[test]
    fn accepts_exact_single_file_inventory() {
        let root = tempfile::tempdir().expect("root");
        let transcript = format!("Create a single file at {}/answer.txt.", root.path().display());

        let result = run(root.path(), &transcript, &[PathBuf::from("answer.txt")]);

        assert_eq!(result.exit_code, Some(0));
        assert!(result.output.contains("Final changed-path inventory: PASS"));
    }

    #[test]
    fn does_not_close_inventory_for_negated_or_ambiguous_language() {
        let root = tempfile::tempdir().expect("root");
        let negated = format!(
            "Do not create a single file; write the result to {}/answer.txt.",
            root.path().display(),
        );
        let multiple = format!(
            "Write a single file at {0}/answer.txt and create {0}/notes.txt.",
            root.path().display(),
        );
        let changed = [PathBuf::from("answer.txt"), PathBuf::from("notes.txt")];

        assert_eq!(run(root.path(), &negated, &changed).exit_code, Some(0));
        assert_eq!(run(root.path(), &multiple, &changed).exit_code, Some(0));
    }
}
