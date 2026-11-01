//! Real C4 router and filesystem read adapter over a disposable Git snapshot.
//!
//! This module lives below `agent_driver` to reuse its committed B0/B1/budget/epoch fixture.

use std::{fs, path::Path, process::Command, sync::Arc, thread, time::Duration};

use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_policy::{OperationClass, OperationDescriptor, OperationRegistry, RiskClass, RiskSet};
use peritus_tool_protocol::{
    BoundedJson, CallLimits, IdempotencyKey, JsonLimits, ResultStatus, ToolCall, ToolResult,
};
use peritus_tool_router::{
    DispatchOutcome, RouterErrorKind, RouterLimits, ToolRegistry, ToolRouter,
};
use peritus_tools_fs::{FsDispatchKind, FsDispatcher, descriptor_catalog};
use peritus_types::{RunId, Sha256Digest};
use peritus_workspace::{
    ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity, WorkspaceBinding,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{authority, contract, identity::CommandIds};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub source: Vec<u8>,
    pub offset: u8,
    pub maximum_bytes: u8,
    pub writer_change: Vec<u8>,
    pub stale_before_first: bool,
}

pub fn strategy(_max_steps: usize) -> BoxedStrategy<Case> {
    (
        prop::collection::vec(any::<u8>(), 0..96),
        0_u8..=96,
        1_u8..=32,
        prop::collection::vec(any::<u8>(), 0..32),
        any::<bool>(),
    )
        .prop_map(|(source, offset, maximum_bytes, writer_change, stale_before_first)| Case {
            offset: offset.min(source.len() as u8),
            source,
            maximum_bytes,
            writer_change,
            stale_before_first,
        })
        .boxed()
}

pub fn examples() -> Vec<Case> {
    vec![
        calibration_case(),
        Case {
            source: Vec::new(),
            offset: 0,
            maximum_bytes: 1,
            writer_change: b"new".to_vec(),
            stale_before_first: true,
        },
        Case {
            source: vec![0, 0xff, 1, 2, 3],
            offset: 1,
            maximum_bytes: 2,
            writer_change: b"changed".to_vec(),
            stale_before_first: false,
        },
        Case {
            source: "é👩‍💻 next".as_bytes().to_vec(),
            offset: 1,
            maximum_bytes: 1,
            writer_change: b"writer".to_vec(),
            stale_before_first: true,
        },
    ]
}

pub fn calibration_case() -> Case {
    Case {
        source: b"abcdef".to_vec(),
        offset: 1,
        maximum_bytes: 3,
        writer_change: b"new writer".to_vec(),
        stale_before_first: true,
    }
}

fn error(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}

fn git(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .map_err(error)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "disposable Git setup {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(char::from(DIGITS[(byte >> 4) as usize]));
        result.push(char::from(DIGITS[(byte & 15) as usize]));
    }
    result
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
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

fn expected_page(case: &Case) -> (&[u8], usize) {
    let start = usize::from(case.offset);
    let end = (start + usize::from(case.maximum_bytes)).min(case.source.len());
    (&case.source[start..end], end)
}

fn verify_result(case: &Case, result: &ToolResult) -> Result<(), String> {
    if result.status() != ResultStatus::Succeeded {
        return Err(format!("C4 filesystem read did not succeed: {result:?}"));
    }
    let output = result.structured().ok_or("C4 filesystem read omitted structured output")?;
    let value: serde_json::Value =
        serde_json::from_slice(output.canonical_bytes()).map_err(error)?;
    let (expected, end) = expected_page(case);
    let (encoding, content) = match std::str::from_utf8(expected) {
        Ok(text) => ("utf8", text.to_owned()),
        Err(_) => ("base64", base64(expected)),
    };
    let next = (end < case.source.len()).then_some(end as u64);
    if value["content"] != content
        || value["encoding"] != encoding
        || value["range_start"] != u64::from(case.offset)
        || value["range_end"] != end as u64
        || value["continuation_offset"] != serde_json::json!(next)
        || value["content_digest"] != digest(expected)
        || value["source_digest"] != digest(&case.source)
        || value["metadata"]["size"] != case.source.len() as u64
        || value["metadata"]["kind"] != "file"
    {
        return Err(format!("C4 filesystem page differs from independent byte model: {value}"));
    }
    Ok(())
}

fn bump(metrics: &mut crate::Metrics, key: &str) {
    *metrics.entry(key.to_owned()).or_default() += 1;
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.source.len() > 96
        || case.writer_change.len() > 32
        || usize::from(case.offset) > case.source.len()
        || case.maximum_bytes == 0
    {
        return Err("setup-incomplete: C4 filesystem case bounds".into());
    }
    let temporary = tempfile::tempdir().map_err(error)?;
    let repository_path = temporary.path().join("repository");
    fs::create_dir(&repository_path).map_err(error)?;
    git(&repository_path, &["init", "-q"])?;
    git(&repository_path, &["config", "user.name", "Peritus Praxis"])?;
    git(&repository_path, &["config", "user.email", "praxis@localhost"])?;
    git(&repository_path, &["config", "core.autocrlf", "false"])?;
    git(&repository_path, &["config", "commit.gpgsign", "false"])?;
    fs::write(repository_path.join("source.bin"), &case.source).map_err(error)?;
    git(&repository_path, &["add", "source.bin"])?;
    git(&repository_path, &["commit", "-q", "--no-gpg-sign", "-m", "Disposable baseline"])?;
    let repository =
        GitRepository::open(RepositoryOptions::new(&repository_path)).map_err(error)?;
    let baseline = repository.resolve_baseline("HEAD").map_err(error)?;
    let snapshot_root = temporary.path().join("snapshot");
    let registration = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("snapshot").map_err(error)?,
            &snapshot_root,
            baseline,
            WorktreeAccess::ReadOnly,
        ))
        .map_err(error)?;

    let run = RunId::new([93; 16]).map_err(error)?;
    let acceptance = contract::command_contract(run, 0)?;
    let mut ids = CommandIds::new(run, 1, &acceptance)?;
    let descriptor = descriptor_catalog()
        .map_err(error)?
        .into_iter()
        .find(|descriptor| descriptor.name().as_str() == "fs.read")
        .ok_or("canonical fs.read descriptor absent")?;
    ids.capability = descriptor.name().clone();
    let identity = SnapshotIdentity::new(
        ids.workspace,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        baseline.commit(),
        baseline.tree(),
    );
    let binding = WorkspaceBinding::new(
        ids.workspace,
        ids.resource,
        ids.environment,
        fs::canonicalize(&repository_path).map_err(error)?,
        baseline.commit(),
        baseline.tree(),
    )
    .map_err(error)?;
    let workspace = Arc::new(
        ReadOnlyWorkspace::open(
            ReadOnlyOpenRequest::new(repository, registration, identity, &repository_path)
                .with_workspace_binding(binding),
        )
        .map_err(error)?,
    );
    let mut writer_change = case.writer_change.clone();
    writer_change.push(0xA5);
    if writer_change == case.source {
        writer_change.push(0x5A);
    }
    fs::write(repository_path.join("source.bin"), writer_change).map_err(error)?;

    let operation = OperationDescriptor::new(
        descriptor.operation().name().clone(),
        descriptor.operation().operation_class(),
        RiskSet::new(descriptor.operation().risks().as_slice().to_vec()).map_err(error)?,
    )
    .map_err(error)?;
    let operations = OperationRegistry::new(vec![operation]).map_err(error)?;
    let descriptor_name = descriptor.name().clone();
    let descriptor_version = descriptor.version();
    let registry = ToolRegistry::new(vec![Arc::new(descriptor)], &operations).map_err(error)?;
    let mut router = ToolRouter::new(registry, RouterLimits::new(1, 4).map_err(error)?);
    let arguments = BoundedJson::parse(
        &serde_json::json!({"path":"source.bin", "offset":case.offset, "maximum_bytes":case.maximum_bytes}).to_string(),
        JsonLimits::PRODUCTION,
    ).map_err(error)?;
    let call = ToolCall::new(
        ids.action,
        descriptor_name,
        descriptor_version,
        arguments,
        CallLimits::new(30_000, 8192, 8192, 8192, 1, 1).map_err(error)?,
        ids.revision,
        authority::instant(30_000),
        IdempotencyKey::new("fs-read-page".to_owned()).map_err(error)?,
    );
    let prepared = router.prepare(call).map_err(error)?;
    let authority = authority::commit_tool_for_operation(
        &temporary.path().join("authority.sqlite"),
        &ids,
        &acceptance,
        &prepared,
        30_000,
        OperationClass::Inspection,
        RiskClass::Read,
    )?;
    let mut dispatcher =
        FsDispatcher::read(FsDispatchKind::Read, Arc::clone(&workspace)).map_err(error)?;
    let mut metrics = crate::Metrics::new();
    bump(&mut metrics, "cases");
    if case.stale_before_first {
        let wrong = authority.request_variant(
            &ids,
            authority::instant(30_000),
            Sha256Digest::new([201; 32]),
            ids.revision.workspace_generation(),
            ids.session,
        );
        let denied = router.dispatch(prepared.clone(), &wrong, &mut dispatcher);
        if !matches!(denied, Err(ref error) if error.kind() == RouterErrorKind::Authorization) {
            return Err(format!("C4 filesystem stale authority was not rejected: {denied:?}"));
        }
        bump(&mut metrics, "stale_authority_denials");
    }
    let request = authority.request(&ids, &prepared);
    let outcome = router.dispatch(prepared.clone(), &request, &mut dispatcher).map_err(error)?;
    let result = match outcome {
        DispatchOutcome::Completed(result) => result,
        DispatchOutcome::Active(handle) => {
            let mut found = None;
            for _ in 0..200 {
                let update = router.poll(handle, authority::instant(10_000)).map_err(error)?;
                if let Some(result) = update.terminal() {
                    found = Some(result.clone());
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            found.ok_or("C4 filesystem read worker did not finish within one second")?
        }
        other => return Err(format!("first C4 filesystem dispatch returned replay: {other:?}")),
    };
    verify_result(case, &result)?;
    bump(&mut metrics, "verified_pages");
    let replay = router.dispatch(prepared, &request, &mut dispatcher).map_err(error)?;
    match replay {
        DispatchOutcome::Replayed(replayed)
            if replayed.canonical_bytes() == result.canonical_bytes() =>
        {
            bump(&mut metrics, "exact_replays");
        }
        other => {
            return Err(format!("C4 filesystem exact replay changed terminal result: {other:?}"));
        }
    }
    if fs::read(snapshot_root.join("source.bin")).map_err(error)? != case.source {
        return Err("writer mutation leaked into C4 filesystem snapshot".into());
    }
    bump(&mut metrics, "writer_isolations");
    Ok(metrics)
}
