//! Router-permit through target-owned C1 filesystem mutation integration tests.

#[path = "fs_dispatch/authority/mod.rs"]
mod authority_support;
#[path = "fs_dispatch/support.rs"]
mod support;

use peritus_patch::{FileMode, LineEndingPolicy, Preimage};
use peritus_tool_router::tool_action_intent;
use peritus_tools_fs::{
    CompiledMutation, CreateInput, FsDispatchKind, PatchEdit, PatchInput, RemoveInput,
    ReplaceInput, WriteInput,
};
use peritus_workspace::patch_authorization_payload_for_caller;
use tempfile::TempDir;

use authority_support::{Ids, workspace_fixture};

#[test]
fn router_dispatches_create_and_write_through_target_owned_gateway() {
    let temp = TempDir::new().expect("temporary root");
    run_create(&temp, "create", "fs.create", FsDispatchKind::Create);
    run_create(&temp, "write", "fs.write", FsDispatchKind::Write);
}

#[test]
fn router_dispatches_remove_replace_and_atomic_multi_file_patch() {
    let temp = TempDir::new().expect("temporary root");
    run_existing(&temp, "remove", "fs.remove", FsDispatchKind::Remove);
    run_existing(&temp, "replace", "fs.replace", FsDispatchKind::Replace);
    run_multi_patch(&temp);
}

#[test]
fn recreated_dispatcher_replays_the_retained_lower_action_outcome() {
    let temp = TempDir::new().expect("temporary root");
    let lower = Ids::new();
    let parent = lower.for_tool_action(92, "fs.create");
    assert_ne!(lower.action, parent.action);
    let fixture = workspace_fixture(&temp, &lower, "mutation-replay");
    let json = final_json("replayed.txt", "once\\n");
    let input = CreateInput::new(
        "replayed.txt",
        b"once\n".to_vec(),
        FileMode::Regular,
        LineEndingPolicy::Preserve,
    )
    .expect("create input");
    let patch = CompiledMutation::create(support::workspace_version(&lower), input)
        .expect("compiled create")
        .into_patch();
    let (router, prepared) = support::prepare(&parent, "fs.create", support::arguments(&json));
    let prepared_digest = prepared.prepared_digest();
    let caller = support::caller(&prepared, &parent);
    assert_eq!(caller.action_id(), parent.action);
    let lower_intent =
        authority_support::intent(&lower, patch_authorization_payload_for_caller(&patch, &caller));
    let lower_receipts = authority_support::receipts(&temp, &lower, &lower_intent);
    let lower_request = authority_support::exact_request(&lower_intent, &lower_receipts, &lower)
        .with_caller_binding(caller);
    let parent_intent = tool_action_intent(
        &prepared,
        parent.actor,
        peritus_policy::ActorRole::Writer,
        parent.environment,
        parent.resource,
    );
    let parent_receipts = authority_support::receipts(&temp, &parent, &parent_intent);
    let parent_request =
        support::tool_request(&parent, &parent_intent, &parent_receipts, &prepared);

    let first = {
        let mut dispatcher = peritus_tools_fs::FsDispatcher::mutation(
            FsDispatchKind::Create,
            std::sync::Arc::clone(&fixture.gateway),
            &lower_request,
        )
        .expect("first dispatcher");
        let outcome =
            support::dispatch_prepared(router, prepared, &parent_request, &mut dispatcher);
        support::assert_success(outcome);
        dispatcher.take_mutation_outcome().expect("first mutation outcome")
    };
    let (lower_consumed, parent_consumed) = {
        let gateway = fixture.gateway.lock().expect("gateway");
        (
            gateway.state().action_consumed(lower.action),
            gateway.state().action_consumed(parent.action),
        )
    };
    assert!(lower_consumed);
    assert!(!parent_consumed);

    let (replay_router, replay_prepared) =
        support::prepare(&parent, "fs.create", support::arguments(&json));
    assert_eq!(replay_prepared.prepared_digest(), prepared_digest);
    let mut replay_dispatcher = peritus_tools_fs::FsDispatcher::mutation(
        FsDispatchKind::Create,
        std::sync::Arc::clone(&fixture.gateway),
        &lower_request,
    )
    .expect("recreated dispatcher");
    let replay = support::dispatch_prepared(
        replay_router,
        replay_prepared,
        &parent_request,
        &mut replay_dispatcher,
    );
    support::assert_success(replay);
    let replayed = replay_dispatcher.take_mutation_outcome().expect("replayed outcome");

    assert_eq!(first.action_id(), lower.action);
    assert_eq!(replayed.action_id(), first.action_id());
    assert_eq!(replayed.patch_identity(), first.patch_identity());
    assert_eq!(
        replayed.applied_patch().installed_manifest(),
        first.applied_patch().installed_manifest(),
    );
    let root = fixture.gateway.lock().expect("gateway").state().binding().root().to_owned();
    assert_eq!(std::fs::read(root.join("replayed.txt")).expect("created once"), b"once\n");
}

#[test]
fn authorized_preimage_conflict_is_failed_without_effect() {
    let temp = TempDir::new().expect("temporary root");
    let lower = Ids::new();
    let parent = lower.for_tool_action(91, "fs.replace");
    let fixture = workspace_fixture(&temp, &lower, "conflict");
    let wrong = Preimage::from_bytes(b"not baseline\n", FileMode::Regular);
    let input = ReplaceInput::new(
        "README.md",
        wrong,
        b"should-not-land\n".to_vec(),
        FileMode::Regular,
        LineEndingPolicy::Preserve,
    )
    .expect("replace input");
    let compiled = CompiledMutation::replace(support::workspace_version(&lower), input)
        .expect("compiled conflict");
    let json = format!(
        r#"{{"content":"should-not-land\n","content_encoding":"utf8","line_endings":"preserve","mode":"regular","path":"README.md","preimage":{}}}"#,
        present_json(b"not baseline\n")
    );
    let (router, prepared) = support::prepare(&parent, "fs.replace", support::arguments(&json));
    let (outcome, mutation) = support::dispatch(
        &temp,
        &lower,
        &parent,
        &fixture.gateway,
        FsDispatchKind::Replace,
        prepared,
        router,
        compiled,
    );
    support::assert_failure(outcome);
    assert!(mutation.is_none());
    assert_eq!(
        std::fs::read(
            fixture.gateway.lock().expect("gateway").state().binding().root().join("README.md"),
        )
        .expect("baseline remains"),
        b"baseline\n"
    );
    assert!(
        !fixture
            .gateway
            .lock()
            .expect("gateway")
            .state()
            .binding()
            .root()
            .join("should-not-land")
            .exists()
    );
}

fn run_create(temp: &TempDir, label: &str, name: &str, kind: FsDispatchKind) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(51, name);
    let fixture = workspace_fixture(temp, &lower, label);
    let (arguments, compiled, path) = if name == "fs.create" {
        let input = CreateInput::new(
            "created.txt",
            b"created\n".to_vec(),
            FileMode::Regular,
            LineEndingPolicy::Preserve,
        )
        .expect("create input");
        (
            final_json("created.txt", "created\\n"),
            CompiledMutation::create(support::workspace_version(&lower), input)
                .expect("compiled create"),
            "created.txt",
        )
    } else {
        let input = WriteInput::new(
            "written.txt",
            Preimage::Absent,
            b"written\n".to_vec(),
            FileMode::Regular,
            LineEndingPolicy::Preserve,
        )
        .expect("write input");
        (
            r#"{"content":"written\n","content_encoding":"utf8","line_endings":"preserve","mode":"regular","path":"written.txt","preimage":{"state":"absent"}}"#
                .to_owned(),
            CompiledMutation::write(support::workspace_version(&lower), input)
                .expect("compiled write"),
            "written.txt",
        )
    };
    let (router, prepared) = support::prepare(&parent, name, support::arguments(&arguments));
    let (outcome, mutation) = support::dispatch(
        temp,
        &lower,
        &parent,
        &fixture.gateway,
        kind,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    assert!(fixture.gateway.lock().expect("gateway").state().binding().root().join(path).is_file());
}

fn run_existing(temp: &TempDir, label: &str, name: &str, kind: FsDispatchKind) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(61, name);
    let fixture = workspace_fixture(temp, &lower, label);
    let preimage = Preimage::from_bytes(b"baseline\n", FileMode::Regular);
    let (arguments, compiled) = if name == "fs.remove" {
        (
            format!(r#"{{"path":"README.md","preimage":{}}}"#, present_json(b"baseline\n")),
            CompiledMutation::remove(
                support::workspace_version(&lower),
                RemoveInput::new("README.md", preimage).expect("remove input"),
            )
            .expect("compiled remove"),
        )
    } else {
        (
            format!(
                r#"{{"content":"replacement\n","content_encoding":"utf8","line_endings":"preserve","mode":"regular","path":"README.md","preimage":{}}}"#,
                present_json(b"baseline\n")
            ),
            CompiledMutation::replace(
                support::workspace_version(&lower),
                ReplaceInput::new(
                    "README.md",
                    preimage,
                    b"replacement\n".to_vec(),
                    FileMode::Regular,
                    LineEndingPolicy::Preserve,
                )
                .expect("replace input"),
            )
            .expect("compiled replace"),
        )
    };
    let (router, prepared) = support::prepare(&parent, name, support::arguments(&arguments));
    let (outcome, mutation) = support::dispatch(
        temp,
        &lower,
        &parent,
        &fixture.gateway,
        kind,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    let path = fixture.gateway.lock().expect("gateway").state().binding().root().join("README.md");
    if name == "fs.remove" {
        assert!(!path.exists());
    } else {
        assert_eq!(std::fs::read(path).expect("replacement"), b"replacement\n");
    }
}

fn run_multi_patch(temp: &TempDir) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(71, "fs.patch");
    let fixture = workspace_fixture(temp, &lower, "multi-patch");
    let preimage = Preimage::from_bytes(b"baseline\n", FileMode::Regular);
    let input = PatchInput::new(vec![
        PatchEdit::Create(
            CreateInput::new(
                "a.txt",
                b"a\n".to_vec(),
                FileMode::Regular,
                LineEndingPolicy::Preserve,
            )
            .expect("create a"),
        ),
        PatchEdit::Create(
            CreateInput::new(
                "b.txt",
                b"b\n".to_vec(),
                FileMode::Regular,
                LineEndingPolicy::Preserve,
            )
            .expect("create b"),
        ),
        PatchEdit::Replace(
            ReplaceInput::new(
                "README.md",
                preimage,
                b"patched\n".to_vec(),
                FileMode::Regular,
                LineEndingPolicy::Preserve,
            )
            .expect("replace baseline"),
        ),
    ])
    .expect("multi patch");
    let compiled =
        CompiledMutation::patch(support::workspace_version(&lower), input).expect("compiled patch");
    let json = format!(
        r#"{{"edits":[{}, {}, {{"content":"patched\n","content_encoding":"utf8","line_endings":"preserve","mode":"regular","operation":"replace","path":"README.md","preimage":{}}}]}}"#,
        edit_create_json("a.txt", "a\\n"),
        edit_create_json("b.txt", "b\\n"),
        present_json(b"baseline\n")
    );
    let (router, prepared) = support::prepare(&parent, "fs.patch", support::arguments(&json));
    let (outcome, mutation) = support::dispatch(
        temp,
        &lower,
        &parent,
        &fixture.gateway,
        FsDispatchKind::Patch,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    let root = fixture.gateway.lock().expect("gateway").state().binding().root().to_owned();
    assert_eq!(std::fs::read(root.join("a.txt")).expect("a"), b"a\n");
    assert_eq!(std::fs::read(root.join("b.txt")).expect("b"), b"b\n");
    assert_eq!(std::fs::read(root.join("README.md")).expect("README"), b"patched\n");
}

fn final_json(path: &str, content: &str) -> String {
    format!(
        r#"{{"content":"{content}","content_encoding":"utf8","line_endings":"preserve","mode":"regular","path":"{path}"}}"#
    )
}

fn edit_create_json(path: &str, content: &str) -> String {
    format!(
        r#"{{"content":"{content}","content_encoding":"utf8","line_endings":"preserve","mode":"regular","operation":"create","path":"{path}"}}"#
    )
}

fn present_json(bytes: &[u8]) -> String {
    format!(
        r#"{{"digest":"{}","mode":"regular","size":{},"state":"present"}}"#,
        digest_hex(peritus_codec::sha256(bytes)),
        bytes.len()
    )
}

fn digest_hex(value: peritus_types::Sha256Digest) -> String {
    let mut output = String::with_capacity(64);
    for byte in value.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("hex rendering");
    }
    output
}
