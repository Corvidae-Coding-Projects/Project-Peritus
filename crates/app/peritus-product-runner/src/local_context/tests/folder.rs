//! In-place working memory retains exact sources without Git or whole-folder snapshots.

use super::*;
use crate::local_context::memory::{LocalMemory, environment::WorkspaceScope};
use std::path::Path;

fn open(root: &Path) -> LocalMemory {
    let state = root.join("private-state");
    std::fs::create_dir_all(&state).unwrap();
    LocalMemory::load_scoped(
        &state.join("memory"),
        root,
        &state.join("run.trace"),
        binding(),
        LocalContextConfig::default(),
        WorkspaceScope { direct: true, protected: vec![state] },
    )
    .unwrap()
}

#[test]
fn folder_memory_reopens_and_file_validity_tracks_only_explicit_dependencies() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("input.txt"), "initial").unwrap();
    let mut memory = open(root.path());
    begin(&mut memory, "folder-one");
    let id = observation(&mut memory, "read", "input.txt contains initial", false);
    let mut proposal = update(memory.model_revision, id, "file-fact", "input.txt contains initial");
    proposal["operations"][0]["validity"] = Value::from("files");
    proposal["operations"][0]["files"] = Value::Array(vec![Value::from("input.txt")]);
    let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
    assert!(result.get("rejected").is_none(), "{result}");
    let view = memory.prepare_view(&profile(32_768), &[]).unwrap();
    memory.publish(&view).unwrap();
    let source_count = memory.sources.len();
    drop(memory);
    let mut memory = open(root.path());
    assert_eq!(memory.sources.len(), source_count);
    assert_eq!(memory.last_view, view);
    std::fs::write(root.path().join("unrelated.txt"), "unrelated").unwrap();
    memory.refresh().unwrap();
    assert_eq!(memory.state.entries(memory.binding).unwrap()[0].status(), WorkingEntryStatus::Open);
    std::fs::write(root.path().join("input.txt"), "changed").unwrap();
    memory.refresh().unwrap();
    assert_eq!(
        memory.state.entries(memory.binding).unwrap()[0].status(),
        WorkingEntryStatus::Stale
    );
    assert!(!root.path().join(".git").exists());
}

#[test]
fn folder_memory_rejects_candidate_claims_and_private_file_dependencies() {
    let root = tempfile::tempdir().unwrap();
    let mut memory = open(root.path());
    begin(&mut memory, "folder-one");
    let id = observation(&mut memory, "read", "public observation", false);
    let mut proposal = update(memory.model_revision, id, "claim", "source-backed claim");
    let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
    assert!(result.get("rejected").is_some(), "{result}");
    proposal["operations"][0]["validity"] = Value::from("files");
    proposal["operations"][0]["files"] =
        Value::Array(vec![Value::from("private-state/memory/owner.lock")]);
    let result = tools::update::execute(&mut memory, proposal.to_string().as_bytes()).unwrap();
    assert!(result.get("rejected").is_some(), "{result}");
    assert!(memory.transcript.files.is_empty());
    assert!(memory.state.entries(memory.binding).unwrap().is_empty());
}
