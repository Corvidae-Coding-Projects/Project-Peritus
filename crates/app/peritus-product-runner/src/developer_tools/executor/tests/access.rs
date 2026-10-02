use super::*;

#[test]
fn folder_private_state_is_excluded_from_read_list_and_search_even_after_contract_refresh() {
    let root = tempfile::tempdir().expect("folder");
    let private = root.path().join("private-state");
    fs::create_dir(&private).expect("private directory");
    fs::write(private.join("secret.txt"), "PRIVATE_CANARY").expect("private fixture");
    fs::write(root.path().join("public.txt"), "public text").expect("public fixture");
    let mut tools = WorkspaceDeveloperTools::read_only(root.path().to_owned())
        .with_protected_paths(&[private])
        .with_task_contract("Inspect this folder.");
    for name in ["workspace_read", "workspace_list", "workspace_search"] {
        let denied = execute(
            &mut tools,
            name,
            r#"{"path":"private-state/secret.txt","query":"PRIVATE_CANARY"}"#,
        );
        assert!(denied.is_error, "{name}");
        assert!(!wire(&denied).contains("PRIVATE_CANARY"));
    }
    let listed = execute(&mut tools, "workspace_list", r#"{"path":"","depth":3}"#);
    assert!(!listed.is_error);
    assert!(!wire(&listed).contains("private-state"));
    let searched =
        execute(&mut tools, "workspace_search", r#"{"path":"","query":"PRIVATE_CANARY"}"#);
    assert!(!searched.is_error);
    assert!(!wire(&searched).contains("PRIVATE_CANARY"));
    let read = execute(&mut tools, "workspace_read", r#"{"path":"public.txt"}"#);
    assert!(!read.is_error);
    assert!(wire(&read).contains("public text"));
}

#[test]
fn read_only_tools_reject_undeclared_mutation_and_process_calls() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "before\n").expect("existing file");
    let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned());

    let listed = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    let read = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"README.md","start_line":1}"#,
    );
    assert!(!listed.is_error);
    assert!(!read.is_error);

    for (name, arguments) in [
        ("workspace_write", r#"{"content":"after\n","path":"README.md"}"#),
        ("workspace_patch", r#"{"new":"after","old":"before","path":"README.md"}"#),
        ("workspace_remove", r#"{"path":"README.md"}"#),
        ("run_command", r#"{"args":["status"],"program":"git"}"#),
        ("command_start", r#"{"args":["status"],"program":"git","purpose":"verification"}"#),
        ("command_poll", r#"{"handle":"active"}"#),
        ("command_stdin", r#"{"handle":"active","text":"input"}"#),
        ("command_resize", r#"{"columns":80,"handle":"active","rows":24}"#),
        ("command_signal", r#"{"handle":"active","signal":"interrupt"}"#),
        ("command_cancel", r#"{"handle":"active"}"#),
        ("command_recover", r#"{"handle":"active"}"#),
    ] {
        let refused = execute(&mut tools, name, arguments);
        assert!(refused.is_error, "{name} must be refused");
        assert!(wire(&refused).contains("read-only workspace access"));
    }
    assert_eq!(
        fs::read_to_string(workspace.path().join("README.md")).expect("unchanged file"),
        "before\n",
    );
}

#[test]
fn explicit_references_are_read_only_and_confined_to_the_named_root() {
    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("reference parent");
    let invoices = references.path().join("Invoices");
    let unmentioned = references.path().join("Private");
    fs::create_dir_all(&invoices).expect("invoice directory");
    fs::create_dir_all(&unmentioned).expect("unmentioned directory");
    let invoice = invoices.join("DS-2026-001.html");
    fs::write(&invoice, "<title>REFERENCE_INVOICE</title>\n").expect("invoice fixture");
    fs::write(unmentioned.join("secret.txt"), "UNMENTIONED_SECRET\n").expect("unmentioned fixture");

    let task = format!("Match the examples in '{}'.", invoices.display());
    let mut tools = writable_tools(workspace.path()).with_reference_contract(&task);
    let listed =
        execute(&mut tools, "workspace_list", &serde_json::json!({"path": invoices}).to_string());
    assert!(!listed.is_error, "{}", wire(&listed));
    let listing: Value = serde_json::from_str(&wire(&listed)).expect("reference listing");
    assert_eq!(listing["reference_root"], invoices.to_string_lossy().as_ref());
    assert!(listing["entries"].as_array().is_some_and(|entries| {
        entries.iter().any(|entry| {
            entry["path"] == invoice.to_string_lossy().as_ref()
                && entry["kind"] == "file"
                && entry["permissions"].is_string()
        })
    }));

    let read =
        execute(&mut tools, "workspace_read", &serde_json::json!({"path": invoice}).to_string());
    assert!(!read.is_error, "{}", wire(&read));
    assert!(wire(&read).contains("REFERENCE_INVOICE"));

    for path in [unmentioned.join("secret.txt"), invoices.join("../Private/secret.txt")] {
        let denied =
            execute(&mut tools, "workspace_read", &serde_json::json!({"path": path}).to_string());
        assert!(denied.is_error, "{}", wire(&denied));
        assert!(!wire(&denied).contains("UNMENTIONED_SECRET"));
    }

    let absolute_write = execute(
        &mut tools,
        "workspace_write",
        &serde_json::json!({"path": invoice, "content": "changed\n"}).to_string(),
    );
    assert!(absolute_write.is_error);
    assert_eq!(
        fs::read_to_string(&invoice).expect("unchanged reference"),
        "<title>REFERENCE_INVOICE</title>\n",
    );
    let ungrounded =
        execute(&mut tools, "workspace_write", r#"{"content":"new\n","path":"new.txt"}"#);
    assert!(ungrounded.is_error, "reference reads must not ground workspace mutation");

    let absolute_workspace = execute(
        &mut tools,
        "workspace_list",
        &serde_json::json!({"path": workspace.path()}).to_string(),
    );
    assert!(absolute_workspace.is_error);
    assert!(wire(&absolute_workspace).contains("workspace-relative"));
    assert!(!wire(&absolute_workspace).contains("no read authority"));
}

#[test]
fn missing_explicit_reference_reports_exact_case_sensitive_path() {
    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let existing = references.path().join("Invoices");
    fs::create_dir(&existing).expect("existing directory");
    fs::write(existing.join("example.txt"), "EXISTING_CASE_CANARY").expect("case fixture");
    let missing = references.path().join("invoices");
    let task = format!("Inspect {}", missing.display());
    let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&task);

    let result =
        execute(&mut tools, "workspace_list", &serde_json::json!({"path": missing}).to_string());
    assert!(result.is_error);
    assert!(wire(&result).contains("not_found"));
    assert!(wire(&result).contains("case-sensitive"));
    assert!(!wire(&result).contains("EXISTING_CASE_CANARY"));
}

#[cfg(unix)]
#[test]
fn explicit_reference_rejects_symbolic_link_targets() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let named = references.path().join("named");
    let outside = references.path().join("outside");
    fs::create_dir(&named).expect("named directory");
    fs::create_dir(&outside).expect("outside directory");
    fs::write(outside.join("secret.txt"), "SYMLINK_SECRET").expect("secret fixture");
    symlink(outside.join("secret.txt"), named.join("file-link")).expect("file link");
    symlink(&outside, named.join("directory-link")).expect("directory link");
    let task = format!("Inspect {}", named.display());
    let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&task);

    for (name, path) in [
        ("workspace_read", named.join("file-link")),
        ("workspace_list", named.join("directory-link")),
    ] {
        let result = execute(&mut tools, name, &serde_json::json!({"path": path}).to_string());
        assert!(result.is_error, "{name}: {}", wire(&result));
        assert!(wire(&result).contains("symbolic"));
        assert!(!wire(&result).contains("SYMLINK_SECRET"));
    }
}
