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
fn explicit_reference_uses_native_rules_for_unique_directory_and_file_casing() {
    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let existing = references.path().join("Documents").join("Invoices");
    fs::create_dir_all(&existing).expect("existing directory");
    let actual_file = existing.join("DS-2026-001.html");
    fs::write(&actual_file, "CASE_CORRECTED_CANARY").expect("case fixture");
    let requested = references.path().join("documents").join("invoices");
    let native_directory_alias = fs::symlink_metadata(&requested).is_ok();
    let mut alias_tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", requested.display()));
    let alias_listing = execute(
        &mut alias_tools,
        "workspace_list",
        &serde_json::json!({"path": requested}).to_string(),
    );
    if native_directory_alias {
        assert!(!alias_listing.is_error, "{}", wire(&alias_listing));
        let listing: Value =
            serde_json::from_str(&wire(&alias_listing)).expect("reference listing");
        assert_eq!(listing["reference_root"], existing.to_string_lossy().as_ref());
    } else {
        assert!(alias_listing.is_error, "{}", wire(&alias_listing));
        assert!(wire(&alias_listing).contains("not_found"));
    }

    let mut exact_tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", existing.display()));
    let exact_listing = execute(
        &mut exact_tools,
        "workspace_list",
        &serde_json::json!({"path": existing}).to_string(),
    );
    assert!(!exact_listing.is_error, "{}", wire(&exact_listing));
    let listing: Value = serde_json::from_str(&wire(&exact_listing)).expect("reference listing");
    assert_eq!(listing["reference_root"], existing.to_string_lossy().as_ref());
    assert!(listing["entries"].as_array().is_some_and(|entries| {
        entries.iter().any(|entry| entry["path"] == actual_file.to_string_lossy().as_ref())
    }));

    let requested_file = existing.join("ds-2026-001.html");
    let native_file_alias = fs::symlink_metadata(&requested_file).is_ok();
    let alias_read = execute(
        &mut exact_tools,
        "workspace_read",
        &serde_json::json!({"path": requested_file}).to_string(),
    );
    if native_file_alias {
        assert!(!alias_read.is_error, "{}", wire(&alias_read));
        let result: Value = serde_json::from_str(&wire(&alias_read)).expect("reference read");
        assert_eq!(result["path"], actual_file.to_string_lossy().as_ref());
    } else {
        assert!(alias_read.is_error, "{}", wire(&alias_read));
        assert!(wire(&alias_read).contains("not_found"));
    }

    let exact_read = execute(
        &mut exact_tools,
        "workspace_read",
        &serde_json::json!({"path": actual_file}).to_string(),
    );
    assert!(!exact_read.is_error, "{}", wire(&exact_read));
    let exact_result: Value = serde_json::from_str(&wire(&exact_read)).expect("reference read");
    assert_eq!(exact_result["content"], "1: CASE_CORRECTED_CANARY");
    assert_eq!(exact_result["path"], actual_file.to_string_lossy().as_ref());
    assert_eq!(exact_result["reference_root"], existing.to_string_lossy().as_ref());

    let missing = execute(
        &mut exact_tools,
        "workspace_read",
        &serde_json::json!({"path": existing.join("missing.txt")}).to_string(),
    );
    assert!(missing.is_error, "{}", wire(&missing));
    assert!(wire(&missing).contains("not_found"));
}

#[cfg(target_os = "linux")]
#[test]
fn explicit_reference_obeys_native_case_resolution_and_prefers_an_exact_name() {
    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let title_case = references.path().join("Invoices");
    let lower_case = references.path().join("invoices");
    fs::create_dir(&title_case).expect("title-case directory");
    fs::create_dir(&lower_case).expect("lower-case directory");
    fs::write(title_case.join("title.txt"), "TITLE_CASE_CANARY").expect("title fixture");
    fs::write(lower_case.join("lower.txt"), "LOWER_CASE_CANARY").expect("lower fixture");

    let ambiguous = references.path().join("INVOICES");
    let mut ambiguous_tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", ambiguous.display()));
    let rejected = execute(
        &mut ambiguous_tools,
        "workspace_list",
        &serde_json::json!({"path": ambiguous}).to_string(),
    );
    assert!(rejected.is_error, "{}", wire(&rejected));
    assert!(wire(&rejected).contains("not_found"));
    assert!(!wire(&rejected).contains("TITLE_CASE_CANARY"));
    assert!(!wire(&rejected).contains("LOWER_CASE_CANARY"));

    let mut exact_tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", title_case.display()));
    let exact = execute(
        &mut exact_tools,
        "workspace_list",
        &serde_json::json!({"path": title_case}).to_string(),
    );
    assert!(!exact.is_error, "{}", wire(&exact));
    assert!(wire(&exact).contains("title.txt"));
    assert!(!wire(&exact).contains("lower.txt"));

    let sibling = execute(
        &mut exact_tools,
        "workspace_read",
        &serde_json::json!({"path": lower_case.join("lower.txt")}).to_string(),
    );
    assert!(sibling.is_error, "{}", wire(&sibling));
    assert!(!wire(&sibling).contains("LOWER_CASE_CANARY"));
}

#[cfg(target_os = "linux")]
#[test]
fn exact_reference_name_wins_among_multiple_case_insensitive_siblings() {
    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let names = ["INVOICES", "Invoices", "invoices"];
    for name in names {
        let directory = references.path().join(name);
        fs::create_dir(&directory).expect("case-sensitive directory");
        fs::write(directory.join("example.txt"), name).expect("distinct contents");
    }
    for name in names {
        let directory = references.path().join(name);
        let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
            .with_reference_contract(&format!("Inspect {}", directory.display()));
        let read = execute(
            &mut tools,
            "workspace_read",
            &serde_json::json!({"path": directory.join("example.txt")}).to_string(),
        );
        assert!(!read.is_error, "{name}: {}", wire(&read));
        let result: Value = serde_json::from_str(&wire(&read)).expect("reference read");
        assert_eq!(result["content"], format!("1: {name}"), "exact name must win: {name}");
        assert_eq!(result["reference_root"], directory.to_string_lossy().as_ref());
    }
}

#[cfg(unix)]
#[test]
fn explicit_reference_allows_an_ancestor_alias_without_allowing_target_links() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().expect("workspace");
    let references = tempfile::tempdir().expect("references");
    let storage = references.path().join("storage");
    let directory = storage.join("Invoices");
    fs::create_dir_all(&directory).expect("reference directory");
    fs::write(directory.join("example.txt"), "ANCESTOR_ALIAS_CANARY").expect("contents");
    let alias = references.path().join("ReferenceAnchor");
    symlink(&storage, &alias).expect("ancestor alias");
    let requested = alias.join("Invoices");
    let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", requested.display()));
    let read = execute(
        &mut tools,
        "workspace_read",
        &serde_json::json!({"path": requested.join("example.txt")}).to_string(),
    );
    assert!(!read.is_error, "{}", wire(&read));
    assert!(wire(&read).contains("ANCESTOR_ALIAS_CANARY"));
    let result: Value = serde_json::from_str(&wire(&read)).expect("reference read");
    assert_eq!(result["reference_root"], alias.join("Invoices").to_string_lossy().as_ref());

    let target = alias.join("target-link");
    symlink(&directory, &target).expect("target link");
    let mut target_tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned())
        .with_reference_contract(&format!("Inspect {}", target.display()));
    let rejected = execute(
        &mut target_tools,
        "workspace_read",
        &serde_json::json!({"path": target.join("example.txt")}).to_string(),
    );
    assert!(rejected.is_error, "{}", wire(&rejected));
    assert!(wire(&rejected).contains("symbolic"));
    assert!(!wire(&rejected).contains("ANCESTOR_ALIAS_CANARY"));
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
