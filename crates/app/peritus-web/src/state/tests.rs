use super::*;

#[test]
fn nesting_rejects_other_roots_and_cycles() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let projects = vec![
        Project {
            id: "a".into(),
            name: "a".into(),
            root: a.path().into(),
            repository: a.path().into(),
            closed: false,
        },
        Project {
            id: "b".into(),
            name: "b".into(),
            root: b.path().into(),
            repository: b.path().into(),
            closed: false,
        },
    ];
    let one = Session {
        settings: crate::sessions::Settings::default(),
        id: "one".into(),
        project: "a".into(),
        parent: None,
        title: "one".into(),
        closed: false,
    };
    let two = Session { id: "two".into(), project: "b".into(), ..one.clone() };
    let child = Session { id: "child".into(), parent: Some("one".into()), ..one.clone() };
    let workspace = Workspace {
        projects,
        sessions: vec![one.clone(), two, child.clone()],
        ..Workspace::default()
    };
    assert!(App::nest(&workspace, &one, Some("two")).is_err());
    assert!(App::nest(&workspace, &one, Some("child")).is_err());
    assert!(App::nest(&workspace, &child, Some("one")).is_ok());
}

#[test]
fn operation_ledger_retires_completed_records_before_pending_records() {
    let mut workspace = Workspace::default();
    for index in 0..=MAX_OPERATION_RECORDS {
        workspace.operations.insert(
            format!("operation-{index:05}"),
            Operation {
                input: serde_json::json!({"command":"test"}),
                result: (index == 0).then(|| serde_json::json!({"ok":true})),
            },
        );
    }

    prune_operations(&mut workspace);

    assert_eq!(workspace.operations.len(), MAX_OPERATION_RECORDS);
    assert!(!workspace.operations.contains_key("operation-00000"));
    assert!(workspace.operations.contains_key("operation-00001"));
}

#[test]
fn operation_ledger_never_discards_an_unresolved_outcome() {
    let mut workspace = Workspace::default();
    for index in 0..MAX_OPERATION_RECORDS {
        workspace.operations.insert(
            format!("operation-{index:05}"),
            Operation { input: serde_json::json!({"command":"test"}), result: None },
        );
    }

    assert!(
        insert_operation(
            &mut workspace,
            "one-too-many".into(),
            serde_json::json!({"command":"test"}),
        )
        .is_err()
    );
    prune_operations(&mut workspace);

    assert_eq!(workspace.operations.len(), MAX_OPERATION_RECORDS);
    assert!(workspace.operations.values().all(|record| record.result.is_none()));
    assert!(!workspace.operations.contains_key("one-too-many"));
}

#[test]
fn startup_recovers_native_operations_that_never_reached_submission() {
    let mut workspace = Workspace::default();
    workspace.operations.insert(
        "send-one".into(),
        Operation { input: serde_json::json!({"command":"send"}), result: None },
    );
    workspace.operations.insert(
        "send-two".into(),
        Operation { input: serde_json::json!({"command":"send"}), result: None },
    );
    workspace.operations.insert(
        "daemon:send-two".into(),
        Operation { input: serde_json::json!({"command":"daemon-request"}), result: None },
    );

    assert!(recover_unsubmitted_native_operations(&mut workspace));
    let recovered = workspace.operations["send-one"].result.as_ref().expect("recovered");
    assert_eq!(recovered["submitted"], false);
    assert_eq!(recovered["retryable"], true);
    assert!(workspace.operations["send-two"].result.is_none());
}

#[test]
fn malformed_workspace_state_is_quarantined_and_does_not_block_open() {
    let root = tempfile::tempdir().expect("root");
    let state_file = root.path().join("state/workspace.json");
    std::fs::create_dir_all(state_file.parent().unwrap()).expect("state directory");
    std::fs::write(&state_file, b"{malformed").expect("malformed state");
    let options = Options {
        port: 4173,
        root: root.path().to_owned(),
        assets: root.path().join("assets"),
        config_file: root.path().join("config/webui.toml"),
        state_file: state_file.clone(),
        daemon_config_root: root.path().join("config"),
        product_state_root: root.path().join("product-state"),
        daemon_config: None,
        endpoint: None,
        cli: PathBuf::from("peritus"),
    };

    let app = App::open(options, 4173).expect("fresh web workspace");

    assert_eq!(app.snapshot().expect("workspace").projects.len(), 1);
    assert!(state_file.parent().unwrap().join(".quarantine/workspace.json.corrupt-0").is_file());
}
