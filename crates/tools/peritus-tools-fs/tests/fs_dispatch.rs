//! Router-permit through target-owned C1 filesystem mutation integration tests.

#[path = "fs_dispatch/authority/mod.rs"]
mod authority_support;
#[path = "fs_dispatch/support.rs"]
mod support;

use peritus_patch::{FileMode, LineEndingPolicy, Preimage};
use peritus_tools_fs::{
    CompiledMutation, CreateInput, FsDispatchKind, PatchEdit, PatchInput, RemoveInput,
    ReplaceInput, WriteInput,
};
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
fn authorized_preimage_conflict_is_failed_without_effect() {
    let temp = TempDir::new().expect("temporary root");
    let lower = Ids::new();
    let parent = lower.for_tool_action(91, "fs.replace");
    let mut fixture = workspace_fixture(&temp, &lower, "conflict");
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
        &mut fixture.gateway,
        FsDispatchKind::Replace,
        prepared,
        router,
        compiled,
    );
    support::assert_failure(outcome);
    assert!(mutation.is_none());
    assert_eq!(
        std::fs::read(fixture.gateway.state().binding().root().join("README.md"))
            .expect("baseline remains"),
        b"baseline\n"
    );
    assert!(!fixture.gateway.state().binding().root().join("should-not-land").exists());
}

fn run_create(temp: &TempDir, label: &str, name: &str, kind: FsDispatchKind) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(51, name);
    let mut fixture = workspace_fixture(temp, &lower, label);
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
        &mut fixture.gateway,
        kind,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    assert!(fixture.gateway.state().binding().root().join(path).is_file());
}

fn run_existing(temp: &TempDir, label: &str, name: &str, kind: FsDispatchKind) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(61, name);
    let mut fixture = workspace_fixture(temp, &lower, label);
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
        &mut fixture.gateway,
        kind,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    let path = fixture.gateway.state().binding().root().join("README.md");
    if name == "fs.remove" {
        assert!(!path.exists());
    } else {
        assert_eq!(std::fs::read(path).expect("replacement"), b"replacement\n");
    }
}

fn run_multi_patch(temp: &TempDir) {
    let lower = Ids::new();
    let parent = lower.for_tool_action(71, "fs.patch");
    let mut fixture = workspace_fixture(temp, &lower, "multi-patch");
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
        &mut fixture.gateway,
        FsDispatchKind::Patch,
        prepared,
        router,
        compiled,
    );
    support::assert_success(outcome);
    assert!(mutation.is_some());
    let root = fixture.gateway.state().binding().root();
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
