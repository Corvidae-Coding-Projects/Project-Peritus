//! Prose, verification and naming cues remain distinct from output requirements.

use super::*;

#[test]
fn arithmetic_addition_does_not_make_a_later_example_input_an_output() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("eval.scm"), "candidate").expect("candidate");
    fs::create_dir(root.path().join("test")).expect("test directory");
    fs::write(root.path().join("test/calculator.scm"), "input").expect("input");
    let transcript = "Write a file eval.scm that is a metacircular evaluator.\n\
        The first example will add 7 and 8 because that is what calculator.scm does.";

    let record = run(root.path(), transcript, &[PathBuf::from("eval.scm")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("Required explicit output paths (1):"));
    assert!(record.output.contains("eval.scm: present"));
    assert!(!record.output.contains("calculator.scm:"));
}

#[test]
fn conditional_troubleshooting_commands_do_not_create_output_requirements() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/review.txt"), "APPROVE\n").expect("review");
    let transcript = "If the tool warns about client.compatibility, run once `tool config \
        --global --add client.compatibility /workspace`. Write the review to `out/review.txt`.";

    let record = run(root.path(), transcript, &[PathBuf::from("out/review.txt")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(
        record
            .output
            .contains(&format!("{}: present", Path::new("out").join("review.txt").display()))
    );
    assert!(!record.output.contains("client.compatibility:"));
}

#[test]
fn conditional_deliverables_are_not_universal_requirements() {
    let root = tempfile::tempdir().expect("root");
    let transcript = "If input rows exist, write `out/rows.csv`. Always create `summary.md`.";

    assert_eq!(required_outputs(root.path(), transcript), vec![PathBuf::from("summary.md")]);
}

#[test]
fn output_verbs_do_not_leak_into_later_verification_sentences() {
    let root = tempfile::tempdir().expect("root");
    let transcript = "Change python_app/app.py HELLO to PYTHON_OK and node_app/app.js HELLO to \
        NODE_OK, update their tests, change only these four files. Run both projects tests/checks \
        and both programs. Run instruction: python python_app/app.py.";
    let record = run(root.path(), transcript, &[]);
    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(!record.output.contains("tests/checks:"));

    // A real output with the same spelling remains mandatory in its own output clause.
    let required = run(root.path(), "Run both projects. Create the file tests/checks.", &[]);
    assert_eq!(required.exit_code, Some(1));
    assert!(required.output.contains(&format!(
        "required explicit output path is missing: {}",
        Path::new("tests").join("checks").display()
    )));
}

#[test]
fn slash_separated_prose_is_not_mistaken_for_an_output_path() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/titles.txt"), "first\nsecond\n").expect("output");
    let transcript = "Write the titles to out/titles.txt: UTF-8, one title per line, with no extra \
        leading/trailing whitespace.";

    let record = run(root.path(), transcript, &[PathBuf::from("out/titles.txt")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(
        record
            .output
            .contains(&format!("{}: present", Path::new("out").join("titles.txt").display()))
    );
    assert!(!record.output.contains("leading/trailing"));
}

#[test]
fn content_qualifiers_do_not_inherit_an_extensionless_output_path_cue() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("report.md"), "report").expect("report");
    for transcript in [
        "Create report.md with start/end times.",
        "Write report.md. Place the result in a new before/after comparison.",
    ] {
        let record = run(root.path(), transcript, &[PathBuf::from("report.md")]);
        assert_eq!(record.exit_code, Some(0), "{}", record.output);
        assert_eq!(required_outputs(root.path(), transcript), vec![PathBuf::from("report.md")]);
    }
}

#[test]
fn extensionless_names_remain_required_after_explicit_file_cues() {
    let root = tempfile::tempdir().expect("root");
    for transcript in [
        "Create the file start/end.",
        "Create a file named start/end.",
        "Write the output to the start/end.",
    ] {
        let record = run(root.path(), transcript, &[]);
        assert_eq!(record.exit_code, Some(1), "{transcript}: {}", record.output);
        assert_eq!(required_outputs(root.path(), transcript), vec![PathBuf::from("start/end")]);
    }
}

#[test]
fn unquoted_extensionless_relative_path_remains_required_in_path_context() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("artifacts/release")).expect("output directory");

    let record = run(root.path(), "Write the output to artifacts/release.", &[]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(
        record
            .output
            .contains(&format!("{}: present", Path::new("artifacts").join("release").display()))
    );
}

#[test]
fn output_format_and_schema_fields_are_not_file_deliverables() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/result.json"), "[]").expect("output");
    for heading in ["Output format:", "Output schema:"] {
        let transcript = format!(
            "Write the answers to `out/result.json`.\n{heading}\n\
             - Each element must include `item_id` and `evidence_hint`.\n\
             - `evidence_hint` must provide a short supporting quote."
        );
        assert_eq!(
            required_outputs(root.path(), &transcript),
            vec![PathBuf::from("out/result.json")],
            "{transcript}"
        );
        assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(0));
    }

    // The same spelling remains mandatory when the user declares an actual file.
    for transcript in [
        "Output files:\n- `evidence_hint`",
        "Produce these schema files:\n- `evidence_hint`",
        "Output format:\n- Create a file named `evidence_hint`.",
    ] {
        assert_eq!(required_outputs(root.path(), transcript), vec![PathBuf::from("evidence_hint")]);
        assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(1));
    }
}

#[test]
fn neighboring_file_references_are_not_additional_output_targets() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("in/parser")).expect("project");
    for name in ["processor.rs", "audit.json"] {
        fs::write(root.path().join("in/parser").join(name), "candidate").expect("candidate");
    }
    let expected =
        vec![PathBuf::from("in/parser/audit.json"), PathBuf::from("in/parser/processor.rs")];
    for relation in ["next to", "adjacent to", "beside", "alongside"] {
        let transcript = format!(
            "Update `{0}/in/parser/processor.rs`.\nWrite `{0}/in/parser/audit.json`.\n\
             The program writes the audit file {relation} `processor.rs`.",
            root.path().display(),
        );
        assert_eq!(required_outputs(root.path(), &transcript), expected, "{transcript}");
        assert_eq!(run(root.path(), &transcript, &expected).exit_code, Some(0));
        // Referencing a neighbor must not suppress a separate explicit output instruction.
        let explicit = format!("{transcript}\nUpdate the file `processor.rs`.");
        assert_eq!(run(root.path(), &explicit, &expected).exit_code, Some(1));
    }
    let neighboring = "Create `audit.json` next to `processor.rs`.";
    assert_eq!(required_outputs(root.path(), neighboring), vec![PathBuf::from("audit.json")]);
    assert_eq!(run(root.path(), neighboring, &[]).exit_code, Some(1));
}
