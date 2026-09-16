//! Literal directory scope survives adjacent Markdown punctuation.

use super::*;

#[test]
fn scoped_output_list_keeps_directory_when_a_quote_touches_an_aside() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("exports")).expect("exports");
    fs::write(root.path().join("exports/summary.json"), "{}").expect("summary");
    fs::write(root.path().join("exports/notes.md"), "notes").expect("notes");
    let expected = vec![PathBuf::from("exports/notes.md"), PathBuf::from("exports/summary.json")];
    for directory in ["exports/".to_owned(), format!("{}/exports/", root.path().display())] {
        for aside in ["—**required**—before returning", "–required–before returning"] {
            let transcript = format!(
                "Write all deliverables under `{directory}`{aside}:\n\
                 1. `summary.json`\n\
                 2. `notes.md`"
            );
            let record = run(root.path(), &transcript, &expected);
            assert_eq!(record.exit_code, Some(0), "{transcript}: {}", record.output);
            assert_eq!(required_outputs(root.path(), &transcript), {
                let mut paths = expected.clone();
                paths.insert(0, PathBuf::from("exports"));
                paths
            });
        }
    }
}

#[test]
fn scoped_output_list_does_not_accept_only_workspace_root_copies() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("exports")).expect("exports");
    fs::write(root.path().join("summary.json"), "{}").expect("wrong-location summary");
    let transcript = "Write the deliverables under `exports/`—required:\n- `summary.json`";
    let record = run(root.path(), transcript, &[PathBuf::from("summary.json")]);
    assert_eq!(record.exit_code, Some(1), "{}", record.output);
    assert!(
        record
            .output
            .contains(&format!("{}: MISSING", Path::new("exports").join("summary.json").display()))
    );
}

#[test]
fn punctuation_inside_a_quoted_directory_remains_part_of_its_identity() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("exports—final")).expect("exports");
    fs::write(root.path().join("exports—final/summary.json"), "{}").expect("summary");
    let transcript = "Write the deliverables under `exports—final/`—required:\n- `summary.json`";
    let record = run(root.path(), transcript, &[]);
    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert_eq!(
        required_outputs(root.path(), transcript),
        vec![PathBuf::from("exports—final"), PathBuf::from("exports—final/summary.json")]
    );
}

#[test]
fn quoted_extensionless_output_with_an_aside_is_still_required() {
    let root = tempfile::tempdir().expect("root");
    for quote in ['`', '\'', '"'] {
        let transcript = format!("Create a file named {quote}GUIDE{quote}—required.");
        assert_eq!(required_outputs(root.path(), &transcript), vec![PathBuf::from("GUIDE")]);
        assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(1));
    }
    fs::write(root.path().join("GUIDE"), "guide").expect("guide");
    assert_eq!(run(root.path(), "Create a file named `GUIDE`—required.", &[]).exit_code, Some(0));
}

#[test]
fn quoted_directory_asides_do_not_override_source_or_conditional_context() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("summary.json"), "{}").expect("summary");
    fs::write(root.path().join("notes.md"), "notes").expect("notes");
    let source = "Produce `summary.json` from `inputs/`—context:\n- `notes.md`";
    assert_eq!(run(root.path(), source, &[]).exit_code, Some(0));
    assert_eq!(
        required_outputs(root.path(), source),
        vec![PathBuf::from("notes.md"), PathBuf::from("summary.json")]
    );
    for instruction in [
        "Read documents under `exports/`—context:",
        "If requested, write files under `exports/`—required:",
        "Do not write files under `exports/`—prohibited:",
    ] {
        let transcript = format!("{instruction}\n- `summary.json`");
        assert!(required_outputs(root.path(), &transcript).is_empty(), "{transcript}");
    }
}

#[test]
fn a_quoted_path_followed_by_a_path_suffix_is_not_shortened() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("summary.json"), "{}").expect("shortened summary");
    for token in ["`summary.json`/child", "`summary.json—draft"] {
        // A slash suffix or unclosed literal must not authorize the shorter file.
        let transcript = format!("Create {token}.");
        assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(1), "{transcript}");
        assert!(
            !required_outputs(root.path(), &transcript).contains(&PathBuf::from("summary.json"))
        );
    }
}
