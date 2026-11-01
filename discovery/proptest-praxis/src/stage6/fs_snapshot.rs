//! Actual Git-backed immutable workspace reads through the filesystem tool service.

use crate::Metrics;
use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_tools_fs::{FileContent, FsReadService, MetadataInput, ReadInput};
use peritus_types::{
    EnvironmentId, Generation, ResourceId, RevisionNumber, Sha256Digest, WorkspaceId,
};
use peritus_workspace::{
    ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity, WorkspaceBinding, WorkspaceEntryKind,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path, process::Command};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub source: Vec<u8>,
    pub page_size: u8,
    pub writer_change: Vec<u8>,
}

fn bump(metrics: &mut Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    (prop::collection::vec(any::<u8>(), 0..96), 1_u8..32, prop::collection::vec(any::<u8>(), 0..96))
        .prop_map(|(source, page_size, writer_change)| Case { source, page_size, writer_change })
        .boxed()
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map_err(|error| format!("disposable Git setup unavailable: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "disposable Git setup {:?} failed: {}",
            arguments,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let packed = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        result.push(char::from(ALPHABET[((packed >> 18) & 63) as usize]));
        result.push(char::from(ALPHABET[((packed >> 12) & 63) as usize]));
        result.push(if chunk.len() > 1 {
            char::from(ALPHABET[((packed >> 6) & 63) as usize])
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            char::from(ALPHABET[(packed & 63) as usize])
        } else {
            '='
        });
    }
    result
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.source.len() > 512 || case.writer_change.len() > 512 || case.page_size == 0 {
        return Err("setup-incomplete: Git-backed filesystem case bounds".into());
    }
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = temporary.path();
    let repository_path = root.join("repository");
    fs::create_dir(&repository_path).map_err(|error| error.to_string())?;
    git(&repository_path, &["init", "-q"])?;
    git(&repository_path, &["config", "user.name", "Peritus Praxis"])?;
    git(&repository_path, &["config", "user.email", "praxis@localhost"])?;
    git(&repository_path, &["config", "core.autocrlf", "false"])?;
    git(&repository_path, &["config", "commit.gpgsign", "false"])?;
    fs::write(repository_path.join("source.bin"), &case.source)
        .map_err(|error| error.to_string())?;
    git(&repository_path, &["add", "source.bin"])?;
    git(&repository_path, &["commit", "-q", "--no-gpg-sign", "-m", "Disposable baseline"])?;
    let repository = GitRepository::open(RepositoryOptions::new(&repository_path))
        .map_err(|error| format!("disposable repository rejected: {error}"))?;
    let baseline = repository.resolve_baseline("HEAD").map_err(|error| error.to_string())?;
    let snapshot_root = root.join("snapshot");
    let registration = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("snapshot").map_err(|error| error.to_string())?,
            &snapshot_root,
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .map_err(|error| format!("disposable snapshot worktree rejected: {error}"))?;
    let workspace_id = WorkspaceId::new([51; 16]).map_err(|error| format!("{error:?}"))?;
    let snapshot_identity = SnapshotIdentity::new(
        workspace_id,
        Generation::first(),
        RevisionNumber::first(),
        baseline.commit(),
        baseline.tree(),
    );
    let wrong = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
        repository.clone(),
        registration.clone(),
        snapshot_identity.clone(),
        &snapshot_root,
    ));
    if wrong.is_ok() {
        return Err("read-only snapshot accepted its own root as writer root".into());
    }
    let binding = WorkspaceBinding::new(
        workspace_id,
        ResourceId::new([52; 16]).map_err(|error| format!("{error:?}"))?,
        EnvironmentId::new([53; 16]).map_err(|error| format!("{error:?}"))?,
        fs::canonicalize(&repository_path).map_err(|error| error.to_string())?,
        baseline.commit(),
        baseline.tree(),
    )
    .map_err(|error| error.to_string())?;
    let workspace = ReadOnlyWorkspace::open(
        ReadOnlyOpenRequest::new(repository, registration, snapshot_identity, &repository_path)
            .with_workspace_binding(binding),
    )
    .map_err(|error| format!("valid read-only snapshot rejected: {error}"))?;
    if workspace.target_binding().is_none()
        || !workspace.inspect().map_err(|error| error.to_string())?.is_clean()
    {
        return Err("snapshot target binding or clean Git state missing".into());
    }
    let service = FsReadService::new(&workspace);
    let metadata = service
        .metadata(&MetadataInput::new("source.bin").map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    if metadata.kind() != WorkspaceEntryKind::File
        || metadata.size() != case.source.len() as u64
        || metadata.executable()
        || metadata.path().as_str() != "source.bin"
    {
        return Err("snapshot file metadata differs from committed fixture".into());
    }
    if ReadInput::range("source.bin", 0, 0).is_ok() {
        return Err("zero-byte page bound was accepted".into());
    }
    let mut metrics = Metrics::new();
    bump(&mut metrics, "cases");
    let mut collected: Vec<u8> = Vec::new();
    let mut offset = 0_u64;
    loop {
        let input = ReadInput::range("source.bin", offset, u64::from(case.page_size))
            .map_err(|error| error.to_string())?;
        let observation = service.read(&input).map_err(|error| error.to_string())?;
        let end = (offset + u64::from(case.page_size)).min(case.source.len() as u64);
        let expected = &case.source[offset as usize..end as usize];
        let content_matches = match observation.content() {
            FileContent::Utf8(text) => {
                std::str::from_utf8(expected).is_ok_and(|valid| valid == text.as_str())
            }
            FileContent::Base64(text) => {
                std::str::from_utf8(expected).is_err() && base64(expected) == text.as_str()
            }
        };
        if !content_matches
            || observation.range() != (offset, end)
            || observation.source_digest() != sha256(&case.source)
            || observation.content_digest() != sha256(expected)
            || observation.continuation_offset() != (end < case.source.len() as u64).then_some(end)
        {
            return Err(format!("Git-backed FS page differs at offset {offset}: {observation:?}"));
        }
        collected.extend(expected);
        bump(&mut metrics, "snapshot_pages");
        if offset == 0 {
            let mut changed = case.writer_change.clone();
            changed.push(0xA5);
            if changed == case.source {
                changed.push(0x5A);
            }
            fs::write(repository_path.join("source.bin"), changed)
                .map_err(|error| error.to_string())?;
            bump(&mut metrics, "writer_changes_after_snapshot");
        }
        match observation.continuation_offset() {
            Some(next) => offset = next,
            None => break,
        }
    }
    if collected != case.source
        || fs::read(snapshot_root.join("source.bin")).map_err(|error| error.to_string())?
            != case.source
        || !workspace.inspect().map_err(|error| error.to_string())?.is_clean()
    {
        return Err("writer change leaked into committed snapshot".into());
    }
    let beyond = ReadInput::range("source.bin", case.source.len() as u64 + 1, 1)
        .map_err(|error| error.to_string())?;
    if service.read(&beyond).is_ok() {
        return Err("read beyond committed source length was accepted".into());
    }
    bump(&mut metrics, "out_of_range_rejections");
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    vec![
        Case { source: Vec::new(), page_size: 1, writer_change: b"changed".to_vec() },
        Case {
            source: b"exact original bytes\n".to_vec(),
            page_size: 5,
            writer_change: b"later".to_vec(),
        },
        Case { source: vec![0, 0xff, 1, 2, 3], page_size: 2, writer_change: vec![9] },
        Case {
            source: "é👩‍💻 next".as_bytes().to_vec(),
            page_size: 1,
            writer_change: b"new".to_vec(),
        },
    ]
}

pub fn calibration_case() -> Case {
    Case { source: b"abcdef".to_vec(), page_size: 2, writer_change: b"writer".to_vec() }
}
