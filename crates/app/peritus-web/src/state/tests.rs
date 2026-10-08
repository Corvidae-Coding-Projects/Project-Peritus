use super::*;

fn options(root: &std::path::Path) -> Options {
    Options {
        port: 4173,
        root: root.to_owned(),
        assets: root.join("assets"),
        config_file: root.join("config/webui.toml"),
        state_file: root.join("state/workspace.json"),
        daemon_config_root: root.join("config"),
        product_state_root: root.join("product-state"),
        daemon_config: None,
        endpoint: None,
        cli: PathBuf::from("peritus"),
    }
}

fn only_entry(directory: &std::path::Path) -> PathBuf {
    let entries = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "expected one operation-store entry");
    entries.into_iter().next().unwrap()
}

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
        conversation: "conversation-one".into(),
        run: "run-one".into(),
        native: None,
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
fn operation_records_are_namespaced_and_first_publication_never_clobbers() {
    let root = tempfile::tempdir().expect("root");
    let state_file = root.path().join("state/workspace.json");
    let first = operation_store::OperationStore::open(&state_file, "workspace-one").unwrap();
    let second = operation_store::OperationStore::open(&state_file, "workspace-two").unwrap();
    first
        .owner("same-operation")
        .unwrap()
        .insert(serde_json::json!({"command":"first"}))
        .unwrap();
    second
        .owner("same-operation")
        .unwrap()
        .insert(serde_json::json!({"command":"second"}))
        .unwrap();

    assert_eq!(first.get("same-operation").unwrap().unwrap().input["command"], "first");
    assert_eq!(second.get("same-operation").unwrap().unwrap().input["command"], "second");
    assert!(
        first
            .owner("same-operation")
            .unwrap()
            .insert(serde_json::json!({"command":"conflict"}))
            .is_err()
    );
}

#[test]
fn pending_pages_are_physically_bounded_by_an_immutable_highwater() {
    let root = tempfile::tempdir().expect("root");
    let store = operation_store::OperationStore::open(
        &root.path().join("state/workspace.json"),
        "workspace",
    )
    .unwrap();
    for index in 0..=operation_store::PAGE_RECORDS {
        store
            .owner(&format!("operation-{index}"))
            .unwrap()
            .insert(serde_json::json!({"command":"test","index":index}))
            .unwrap();
    }
    let first = store.pending_page(None, None).unwrap();
    assert_eq!(first.operations.len(), operation_store::PAGE_RECORDS);
    assert!(first.cursor.is_some());
    store
        .owner("later-operation")
        .unwrap()
        .insert(serde_json::json!({"command":"test"}))
        .unwrap();
    let second = store
        .pending_page(first.cursor.as_deref(), Some(&first.snapshot))
        .unwrap();
    assert_eq!(second.operations.len(), 1);
    assert_eq!(second.operations[0].0, "operation-128");
    assert!(second.cursor.is_none());

    let stable = operation_store::OperationStore::open(
        &root.path().join("state/workspace.json"),
        "workspace",
    )
    .unwrap();
    assert!(
        stable
            .pending_page(first.cursor.as_deref(), Some(&first.snapshot))
            .is_ok(),
        "ordinary reconciliation must retain the index generation"
    );

    let namespace = only_entry(&root.path().join("state/workspace.operations"));
    std::fs::remove_file(namespace.join("pending-index.sqlite3")).unwrap();
    let rebuilt = operation_store::OperationStore::open(
        &root.path().join("state/workspace.json"),
        "workspace",
    )
    .unwrap();
    let error = rebuilt
        .pending_page(first.cursor.as_deref(), Some(&first.snapshot))
        .err()
        .expect("a rebuilt index rejects its old continuation");
    assert!(error.0.contains("obsolete index generation"));
}

#[test]
fn index_only_operation_identity_remains_reviewable_without_inventing_input() {
    let root = tempfile::tempdir().expect("root");
    let state_file = root.path().join("state/workspace.json");
    let store = operation_store::OperationStore::open(&state_file, "workspace").unwrap();
    store
        .owner("lost-operation")
        .unwrap()
        .insert(serde_json::json!({"command":"send","text":"lost"}))
        .unwrap();
    let namespace = only_entry(&root.path().join("state/workspace.operations"));
    let pending = only_entry(&namespace.join("pending"));
    std::fs::remove_file(pending).unwrap();

    let reopened = operation_store::OperationStore::open(&state_file, "workspace").unwrap();
    let page = reopened.pending_page(None, None).unwrap();
    assert_eq!(page.operations.len(), 1);
    assert_eq!(page.operations[0].0, "lost-operation");
    assert_eq!(page.operations[0].1["command"], "recovery-unknown");
    assert!(page.operations[0].1.get("text").is_none());

    reopened
        .owner("lost-operation")
        .unwrap()
        .acknowledge_missing_evidence(serde_json::json!({
            "reviewed":true,
            "acceptance":null
        }))
        .unwrap();
    let receipt = reopened.get("lost-operation").unwrap().unwrap();
    assert_eq!(receipt.input["command"], "recovery-unknown");
    assert_eq!(receipt.result.unwrap()["acceptance"], serde_json::Value::Null);
    assert!(reopened.pending_page(None, None).unwrap().operations.is_empty());
}

#[test]
fn legacy_operations_migrate_before_metadata_drops_the_embedded_ledger() {
    let root = tempfile::tempdir().expect("root");
    let options = options(root.path());
    let state_file = options.state_file.clone();
    std::fs::create_dir_all(options.state_file.parent().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "identity":"workspace",
        "projects":[],
        "sessions":[],
        "operations":{
            "retained":{"input":{"command":"config"},"prepared":null,"result":null}
        },
        "attachments":{}
    });
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&options.state_file, &bytes).unwrap();

    let app = App::open(options, 4173).expect("migrated workspace");

    assert_eq!(app.operation("retained").unwrap().unwrap().input["command"], "config");
    let published: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_file).unwrap()).unwrap();
    assert_eq!(published["schema_version"], 2);
    assert!(published["workspace"].get("operations").is_none());
    let legacy_digest = crate::files::revision(&bytes);
    assert!(
        state_file
            .parent()
            .unwrap()
            .join(format!("workspace-migrations/{legacy_digest}.legacy.json"))
            .is_file()
    );
}

#[test]
fn malformed_workspace_state_blocks_open_without_replacing_the_original() {
    let root = tempfile::tempdir().expect("root");
    let options = options(root.path());
    let state_file = options.state_file.clone();
    std::fs::create_dir_all(options.state_file.parent().unwrap()).unwrap();
    let original = b"{malformed";
    std::fs::write(&options.state_file, original).unwrap();

    assert!(App::open(options, 4173).is_err());
    assert_eq!(std::fs::read(state_file).unwrap(), original);
}

#[test]
fn startup_settles_only_native_operations_that_never_reached_submission() {
    let root = tempfile::tempdir().expect("root");
    let app = App::open(options(root.path()), 4173).unwrap();
    app.record_operation(
        "send-one".into(),
        serde_json::json!({"command":"send"}),
    )
    .unwrap();
    app.record_operation(
        "send-two".into(),
        serde_json::json!({"command":"send"}),
    )
    .unwrap();
    app.record_operation(
        "daemon:send-two:workbench:queue".into(),
        serde_json::json!({"command":"daemon-request"}),
    )
    .unwrap();
    drop(app);

    let reopened = App::open(options(root.path()), 4173).unwrap();

    let recovered = reopened.operation("send-one").unwrap().unwrap().result.unwrap();
    assert_eq!(recovered["submitted"], false);
    assert_eq!(recovered["retryable"], true);
    assert!(reopened.operation("send-two").unwrap().unwrap().result.is_none());
}
