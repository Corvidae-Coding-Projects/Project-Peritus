use super::*;

#[test]
fn missing_nested_output_rejects_same_basename_at_workspace_root() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("main.py.c"), "candidate").expect("candidate");
    let transcript = format!(
        "Write me a single file in {}/polyglot/main.py.c which is a polyglot.",
        root.path().display(),
    );

    let record = run(root.path(), &transcript, &[PathBuf::from("main.py.c")]);

    assert_eq!(record.exit_code, Some(1));
    assert!(record.output.contains("required explicit output path is missing"));
    assert!(record.output.contains("candidate main.py.c has the requested basename"));
}

#[test]
fn exact_output_passes_without_treating_command_products_as_required() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("polyglot")).expect("directory");
    fs::write(root.path().join("polyglot/main.py.c"), "candidate").expect("candidate");
    let transcript = format!(
        "Write a file in {0}/polyglot/main.py.c.\nRun gcc {0}/polyglot/main.py.c -o {0}/polyglot/cmain.",
        root.path().display(),
    );

    let record = run(root.path(), &transcript, &[PathBuf::from("polyglot/main.py.c")]);

    assert_eq!(record.exit_code, Some(0));
    assert!(record.output.contains("polyglot/main.py.c: present"));
    assert!(!record.output.contains("cmain: present"));
}

#[test]
fn read_only_and_negated_paths_are_not_required_outputs() {
    let root = tempfile::tempdir().expect("root");
    let transcript = format!(
        "Read {0}/input.json. Do not modify {0}/locked.json. Write the result to out/report.json.",
        root.path().display(),
    );
    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/report.json"), "{}").expect("output");

    let mentions = extract(root.path(), &transcript);

    assert!(
        mentions.contains(&PathMention {
            relative: PathBuf::from("input.json"),
            required_output: false,
        })
    );
    assert!(
        mentions.contains(&PathMention {
            relative: PathBuf::from("locked.json"),
            required_output: false,
        })
    );
    assert!(mentions.contains(&PathMention {
        relative: PathBuf::from("out/report.json"),
        required_output: true,
    }));
}

#[test]
fn quoted_extensionless_executable_is_a_required_output() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("cli_tool"), "candidate").expect("executable");
    let transcript = "The final output must include a binary executable called `cli_tool`.";

    let record = run(root.path(), transcript, &[PathBuf::from("cli_tool")]);

    assert_eq!(record.exit_code, Some(0));
    assert!(record.output.contains("cli_tool: present"));
}

#[test]
fn command_interpreter_is_not_mistaken_for_an_output_path() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("extract.js"), "candidate").expect("program");
    fs::write(root.path().join("out.json"), "{}").expect("output");
    let transcript = "Write a program and run it as `node extract.js /app/a.out > out.json`.";

    let record =
        run(root.path(), transcript, &[PathBuf::from("extract.js"), PathBuf::from("out.json")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("extract.js: present"));
    assert!(record.output.contains("out.json: present"));
    assert!(!record.output.contains("node:"));
}

#[test]
fn attendee_email_is_not_mistaken_for_an_output_path() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("meeting_scheduled.ics"), "candidate").expect("calendar");
    let transcript =
        "Create meeting_scheduled.ics for Alice (alice@example.com). Then send the invitation.";

    let record = run(root.path(), transcript, &[PathBuf::from("meeting_scheduled.ics")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("meeting_scheduled.ics: present"));
    assert!(!record.output.contains("alice@example.com:"));

    fs::write(root.path().join("alice@example.com"), "candidate").expect("quoted file");
    let explicitly_named = run(
        root.path(),
        "Create the file `alice@example.com`.",
        &[PathBuf::from("alice@example.com")],
    );
    assert_eq!(explicitly_named.exit_code, Some(0), "{}", explicitly_named.output);
    assert!(explicitly_named.output.contains("alice@example.com: present"));
}

#[test]
fn prose_abbreviation_is_not_an_output_path() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("answer.txt"), "1000000\n").expect("answer");
    let transcript = format!(
        "Write the integer without commas (e.g. \"1000000\") to {}/answer.txt.",
        root.path().display(),
    );

    let record = run(root.path(), &transcript, &[PathBuf::from("answer.txt")]);

    assert_eq!(record.exit_code, Some(0));
    assert!(record.output.contains("Required explicit output paths (1):"));
    assert!(record.output.contains("answer.txt: present"));
    assert!(!record.output.contains("e.g"));
}
