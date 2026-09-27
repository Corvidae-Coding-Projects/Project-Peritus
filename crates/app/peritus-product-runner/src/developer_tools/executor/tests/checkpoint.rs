//! Fail-closed ordering at the user-checkpoint boundary before workspace effects.

use super::*;
use crate::workspace_delivery::scope::ScopedBaseline;
use crate::{WorkspaceMutationKind, developer_tools::ToolCheckpointBoundary};
use std::sync::{Arc, Mutex};

#[test]
fn changed_write_captures_the_exact_preimage_before_the_effect() {
    let workspace = tempfile::tempdir().expect("workspace");
    let target = workspace.path().join("artifact.txt");
    fs::write(&target, "before\n").expect("fixture");
    let observations = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&observations);
    let observed_target = target.clone();
    let observer = Arc::new(move |boundary| {
        let preimage = if matches!(boundary, ToolCheckpointBoundary::BeforeMutation { .. }) {
            Some(fs::read(&observed_target).map_err(|error| error.to_string())?)
        } else {
            None
        };
        captured
            .lock()
            .map_err(|_| "observation lock poisoned".to_owned())?
            .push((boundary, preimage));
        Ok(())
    });
    let mut tools = writable_tools(workspace.path()).with_checkpoint_observer(observer);
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"artifact.txt","start_line":1}"#,
    );

    let result =
        execute(&mut tools, "workspace_write", r#"{"content":"after\n","path":"artifact.txt"}"#);

    assert!(!result.is_error, "{}", wire(&result));
    assert_eq!(fs::read(&target).expect("result"), b"after\n");
    let observations = observations.lock().expect("observations");
    assert_eq!(observations.len(), 2);
    assert_eq!(
        observations[0],
        (
            ToolCheckpointBoundary::BeforeMutation {
                path: "artifact.txt".to_owned(),
                kind: WorkspaceMutationKind::File,
            },
            Some(b"before\n".to_vec()),
        )
    );
    assert!(matches!(
        &observations[1].0,
        ToolCheckpointBoundary::Mutation {
            path,
            kind: WorkspaceMutationKind::File,
            owned_postchange: crate::control::CheckpointFileVersion::Present { bytes: 6, .. },
        } if path == "artifact.txt"
    ));
    drop(observations);
}

#[test]
fn checkpoint_failure_aborts_the_workspace_effect_and_unchanged_writes_skip_capture() {
    let workspace = tempfile::tempdir().expect("workspace");
    let target = workspace.path().join("artifact.txt");
    fs::write(&target, "stable\n").expect("fixture");
    let calls = Arc::new(Mutex::new(0_u8));
    let counter = Arc::clone(&calls);
    let observer = Arc::new(move |boundary| {
        if matches!(boundary, ToolCheckpointBoundary::BeforeMutation { .. }) {
            *counter.lock().map_err(|_| "counter lock poisoned".to_owned())? += 1;
            return Err("checkpoint storage full".to_owned());
        }
        Ok(())
    });
    let mut tools = writable_tools(workspace.path()).with_checkpoint_observer(observer);
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"artifact.txt","start_line":1}"#,
    );
    let unchanged =
        execute(&mut tools, "workspace_write", r#"{"content":"stable\n","path":"artifact.txt"}"#);
    assert!(!unchanged.is_error, "{}", wire(&unchanged));
    assert_eq!(*calls.lock().expect("calls"), 0);

    let refused =
        execute(&mut tools, "workspace_write", r#"{"content":"changed\n","path":"artifact.txt"}"#);
    assert!(refused.is_error);
    assert!(wire(&refused).contains("checkpoint storage full"));
    assert_eq!(fs::read(&target).expect("preserved"), b"stable\n");
    assert_eq!(*calls.lock().expect("calls"), 1);
}

#[test]
fn completed_effect_with_failed_post_checkpoint_is_finalized_on_replay() {
    use crate::developer_tools::receipt::{EffectReceiptLedger, ReceiptDecision};

    let workspace = tempfile::tempdir().expect("workspace");
    let receipts = receipt_path(workspace.path());
    let ownership = WorkspaceOwnership::capture(workspace.path());
    let call = completed_call(
        "checkpoint-recovery",
        "workspace_write",
        r#"{"content":"durable\n","path":"artifact.txt"}"#,
    );
    let observer = Arc::new(move |boundary| {
        if matches!(boundary, ToolCheckpointBoundary::Mutation { .. }) {
            Err("checkpoint storage full after effect".to_owned())
        } else {
            Ok(())
        }
    });
    let mut first = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        ownership.clone(),
        receipts.clone(),
        "checkpoint-recovery".to_owned(),
        Duration::from_secs(30),
        test_command_runtime(workspace.path()),
    )
    .with_checkpoint_observer(observer);
    let _ = execute(&mut first, "workspace_list", r#"{"depth":1,"path":""}"#);

    let Err(error) = first.execute(&call) else {
        panic!("post-effect checkpoint failure was hidden");
    };

    assert!(error.to_string().contains("checkpoint storage full after effect"));
    assert_eq!(fs::read(workspace.path().join("artifact.txt")).unwrap(), b"durable\n");
    let recovered_events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&recovered_events);
    let recovery_observer = Arc::new(move |boundary| {
        captured.lock().map_err(|_| "event lock poisoned".to_owned())?.push(boundary);
        Ok(())
    });
    let mut recovered = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        ownership,
        receipts.clone(),
        "checkpoint-recovery".to_owned(),
        Duration::from_secs(30),
        test_command_runtime(workspace.path()),
    )
    .with_checkpoint_observer(recovery_observer);

    let replay = recovered.execute(&call).expect("checkpoint recovery replay");

    assert!(!replay.is_error);
    assert!(matches!(
        recovered_events.lock().unwrap().as_slice(),
        [ToolCheckpointBoundary::Mutation { path, .. }] if path == "artifact.txt"
    ));
    let mut ledger = EffectReceiptLedger::new(receipts, "checkpoint-recovery".to_owned());
    assert!(matches!(
        ledger.begin(&call).expect("finalized receipt"),
        ReceiptDecision::Replay { is_error: false, .. }
    ));
}

#[test]
fn empty_directory_capture_precedes_removal_with_an_explicit_kind() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::create_dir(workspace.path().join("empty")).expect("directory");
    fs::write(workspace.path().join("anchor.txt"), "grounding\n").expect("anchor");
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let observer = Arc::new(move |boundary| {
        captured.lock().map_err(|_| "observation lock poisoned".to_owned())?.push(boundary);
        Ok(())
    });
    let mut tools = writable_tools(workspace.path()).with_checkpoint_observer(observer);
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"anchor.txt","start_line":1}"#,
    );

    let removed = execute(&mut tools, "workspace_remove", r#"{"path":"empty"}"#);

    assert!(!removed.is_error, "{}", wire(&removed));
    assert!(!workspace.path().join("empty").exists());
    let recorded = events.lock().expect("observed");
    assert_eq!(
        recorded.first(),
        Some(&ToolCheckpointBoundary::BeforeMutation {
            path: "empty".to_owned(),
            kind: WorkspaceMutationKind::EmptyDirectory,
        })
    );
    assert!(matches!(
        recorded.last(),
        Some(ToolCheckpointBoundary::Mutation {
            path,
            kind: WorkspaceMutationKind::EmptyDirectory,
            owned_postchange: crate::control::CheckpointFileVersion::Absent,
        }) if path == "empty"
    ));
    drop(recorded);
}

#[test]
fn intervening_change_that_refuses_patch_never_reports_an_owned_mutation() {
    let workspace = tempfile::tempdir().expect("workspace");
    let target = workspace.path().join("artifact.txt");
    fs::write(&target, "before\n").expect("fixture");
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let changed_target = target.clone();
    let observer = Arc::new(move |boundary| {
        if matches!(boundary, ToolCheckpointBoundary::BeforeMutation { .. }) {
            fs::write(&changed_target, "independent\n").map_err(|error| error.to_string())?;
        }
        captured.lock().map_err(|_| "event lock poisoned".to_owned())?.push(boundary);
        Ok(())
    });
    let mut tools = writable_tools(workspace.path()).with_checkpoint_observer(observer);
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"artifact.txt","start_line":1}"#,
    );

    let refused = execute(
        &mut tools,
        "workspace_patch",
        r#"{"new":"after","old":"before","path":"artifact.txt"}"#,
    );

    assert!(refused.is_error);
    assert_eq!(fs::read(&target).expect("preserved"), b"independent\n");
    let events = events.lock().expect("events");
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], ToolCheckpointBoundary::BeforeMutation { .. }));
    drop(events);
}

#[test]
fn successful_command_seals_only_changed_scoped_paths_from_its_exact_receipt() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = tempfile::tempdir().expect("state");
    fs::write(workspace.path().join(".peritus-create-output-fixture"), "run")
        .expect("command marker");
    fs::write(workspace.path().join("stable.txt"), "stable\n").expect("stable fixture");
    let scope = ScopedBaseline::new(
        workspace.path().to_owned(),
        state.path().join("scope.jsonl"),
        Vec::new(),
        1,
    );
    scope.enroll("command-output.txt").expect("changed scope");
    scope.enroll("stable.txt").expect("stable scope");
    let events = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let observer = Arc::new(move |boundary| {
        captured.lock().map_err(|_| "event lock poisoned".to_owned())?.push(boundary);
        Ok(())
    });
    let mut tools = writable_tools(workspace.path())
        .with_in_place_scope(Some(scope))
        .with_checkpoint_observer(observer);
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"stable.txt","start_line":1}"#,
    );
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let result = execute(
        &mut tools,
        "run_command",
        &format!(
            r#"{{"args":["--exact","developer_tools::executor::tests::command::command_created_file_fixture","--nocapture"],"cwd":".","program":{program},"purpose":"verification","timeout_seconds":10}}"#
        ),
    );

    assert!(!result.is_error, "{}", wire(&result));
    let events = events.lock().expect("events");
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ToolCheckpointBoundary::BeforeMutation { .. }))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, ToolCheckpointBoundary::Mutation { .. }))
            .count(),
        1
    );
    assert!(events.iter().any(|event| matches!(
        event,
        ToolCheckpointBoundary::Mutation {
            path,
            kind: WorkspaceMutationKind::File,
            owned_postchange: crate::control::CheckpointFileVersion::Present { bytes: 21, .. },
        } if path == "command-output.txt"
    )));
    drop(events);
}
