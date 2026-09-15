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
    assert!(
        record
            .output
            .contains(&format!("{}: present", Path::new("polyglot").join("main.py.c").display()))
    );
    assert!(!record.output.contains("cmain: present"));
}

#[test]
fn output_declaration_requires_literal_paths_in_following_list_items() {
    let root = tempfile::tempdir().expect("root");
    let transcript = format!(
        "After completing the task, produce the following:\n\n\
         - `{0}/out/report.md`: the final report.\n\
         - `{0}/out/progress.md`: the execution record.",
        root.path().display(),
    );
    let expected = vec![PathBuf::from("out/progress.md"), PathBuf::from("out/report.md")];
    assert_eq!(required_outputs(root.path(), &transcript), expected);
    assert_eq!(run(root.path(), &transcript, &[]).exit_code, Some(1));

    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/report.md"), "report").expect("report");
    fs::write(root.path().join("out/progress.md"), "progress").expect("progress");
    assert_eq!(run(root.path(), &transcript, &expected).exit_code, Some(0));
}

#[test]
fn output_list_context_does_not_require_description_inputs_or_later_read_only_lists() {
    let root = tempfile::tempdir().expect("root");
    let transcript = format!(
        "Produce the following:\n\
         - `{0}/out/report.md`: summarize `{0}/in/source.txt`.\n\n\
         ## Read-only inputs\n\
         - `{0}/in/other.txt`: do not change it.",
        root.path().display(),
    );
    assert_eq!(required_outputs(root.path(), &transcript), vec![PathBuf::from("out/report.md")]);
}

#[test]
fn conditional_output_declaration_does_not_make_its_list_unconditional() {
    let root = tempfile::tempdir().expect("root");
    let transcript = "If source rows exist, produce the following:\n- `out/optional.csv`: rows.";
    assert!(required_outputs(root.path(), transcript).is_empty());
    assert_eq!(run(root.path(), transcript, &[]).exit_code, Some(0));
}

#[test]
fn output_list_filenames_use_the_declared_directory() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir_all(root.path().join("out/nested")).expect("output directories");
    fs::write(root.path().join("out/report.json"), "{}").expect("report");
    fs::write(root.path().join("out/nested/notes.md"), "notes").expect("notes");
    fs::write(root.path().join("out/exact.json"), "{}").expect("exact report");
    fs::write(root.path().join("out/full.json"), "{}").expect("qualified report");
    let expected = vec![
        PathBuf::from("out/exact.json"),
        PathBuf::from("out/full.json"),
        PathBuf::from("out/nested/notes.md"),
        PathBuf::from("out/report.json"),
    ];
    for directory in ["out/".to_owned(), format!("{}/out/", root.path().display())] {
        let transcript = format!(
            "Produce the following artifacts, all written to `{directory}`:\n\n\
             1. `report.json`\n\
             2. `nested/notes.md`\n\
             3. `{}/out/exact.json`\n\
             4. `out/full.json`",
            root.path().display(),
        );
        let record = run(root.path(), &transcript, &expected);
        assert_eq!(record.exit_code, Some(0), "{}", record.output);
        assert_eq!(required_outputs(root.path(), &transcript), {
            let mut required = expected.clone();
            required.insert(0, PathBuf::from("out"));
            required
        });
    }
}

#[test]
fn source_directory_does_not_become_the_output_list_directory() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("report.json"), "{}").expect("report");
    fs::write(root.path().join("notes.md"), "notes").expect("notes");
    let transcript = "Produce report.json from `in/`:\n- `notes.md`";
    let record =
        run(root.path(), transcript, &[PathBuf::from("report.json"), PathBuf::from("notes.md")]);
    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert_eq!(
        required_outputs(root.path(), transcript),
        vec![PathBuf::from("notes.md"), PathBuf::from("report.json")]
    );
}

#[test]
fn nested_schema_fields_are_not_output_list_files() {
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("out")).expect("output directory");
    fs::write(root.path().join("out/report.json"), "{}").expect("report");
    fs::write(root.path().join("out/notes.md"), "notes").expect("notes");
    let transcript = "Produce the following files, all written to `out/`:\n\
        1. `report.json`\n   - `status`: a string field.\n   - `items`: an array field.\n\
        2. `notes.md`";
    let expected =
        vec![PathBuf::from("out"), PathBuf::from("out/notes.md"), PathBuf::from("out/report.json")];
    let record = run(root.path(), transcript, &expected[1..]);
    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert_eq!(required_outputs(root.path(), transcript), expected);
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

    let mentions = extract(root.path(), &transcript).mentions;

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
fn alternative_output_paths_require_one_member_instead_of_every_member() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("ars.R"), "candidate").expect("implementation");
    fs::write(root.path().join("normal_samples.txt"), "samples").expect("samples");
    let transcript = format!(
        "Save the main implementation in {0}/ars.R. Generate at least one sample file named \
         {0}/normal_samples.txt or {0}/exponential_samples.txt.",
        root.path().display(),
    );
    let changed = [PathBuf::from("ars.R"), PathBuf::from("normal_samples.txt")];

    let record = run(root.path(), &transcript, &changed);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("one of [normal_samples.txt: present"));
    assert!(
        !record
            .output
            .contains("required explicit output path is missing: exponential_samples.txt")
    );
    assert_eq!(required_outputs(root.path(), &transcript), vec![PathBuf::from("ars.R")]);

    fs::remove_file(root.path().join("normal_samples.txt")).expect("remove samples");
    let missing = run(root.path(), &transcript, &[PathBuf::from("ars.R")]);
    assert_eq!(missing.exit_code, Some(1));
    assert!(missing.output.contains("at least one alternative explicit output path is required"));
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

#[test]
fn descriptive_extension_is_not_a_literal_path_but_a_dotfile_is() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("program.py"), "candidate").expect("program");
    fs::write(root.path().join(".env"), "MODE=test\n").expect("dotfile");
    let transcript = format!(
        "Create {0}/program.py to modify the .DAT files. Create the .env file.",
        root.path().display(),
    );

    let record =
        run(root.path(), &transcript, &[PathBuf::from("program.py"), PathBuf::from(".env")]);

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("program.py: present"));
    assert!(record.output.contains(".env: present"));
    assert!(!record.output.contains(".DAT:"));
}

#[test]
fn generated_filename_placeholders_are_not_literal_output_paths() {
    let root = tempfile::tempdir().expect("root");
    for path in ["kv-store.proto", "kv_store_pb2.py", "kv_store_pb2_grpc.py", "server.py"] {
        fs::write(root.path().join(path), "candidate").expect("candidate");
    }
    let transcript = format!(
        "Create {0}/kv-store.proto. Generate two files: {{class name}}_pb2.py and \
         <class name>_pb2_grpc.py, and place them in {0}. Create {0}/server.py.",
        root.path().display(),
    );

    let record = run(
        root.path(),
        &transcript,
        &[
            PathBuf::from("kv-store.proto"),
            PathBuf::from("kv_store_pb2.py"),
            PathBuf::from("kv_store_pb2_grpc.py"),
            PathBuf::from("server.py"),
        ],
    );

    assert_eq!(record.exit_code, Some(0), "{}", record.output);
    assert!(record.output.contains("kv-store.proto: present"));
    assert!(record.output.contains("server.py: present"));
    assert!(!record.output.contains("name}_pb2.py"));
    assert!(!record.output.contains("name>_pb2_grpc.py"));
}
