//! Real patch transactions checked against a small independent file-tree model.
//!
//! Each case owns a disposable workspace and a disjoint transaction root. Generated actions
//! include successful multi-file changes, stale bindings, changed preimages, cancellation, and
//! completion loss followed by exact-binding recovery. No production data is touched.

use crate::Metrics;
use peritus_patch::{
    ErrorCode, FileMode, FinalFile, LineEndingPolicy, PatchOperation, PatchSet, Preimage,
    RecoveryBinding, RecoveryState, WorkspacePath, apply_patch_with_completion,
    apply_patch_with_completion_and_cancellation, recover_transaction,
};
use peritus_types::{Generation, RevisionNumber, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

const PATHS: [&str; 4] = ["a.txt", "docs/b.txt", "deep/nested/c.bin", "unicode/é.txt"];
const OUTSIDE_CANARY: &[u8] = b"sibling-owned-byte-canary";
const INSIDE_CANARY: &[u8] = b"workspace-unrelated-byte-canary";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub initial: Vec<Option<Blob>>,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blob {
    pub bytes: Vec<u8>,
    pub executable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub slot: u8,
    pub count: u8,
    pub delete_present: bool,
    pub payload: Vec<u8>,
    pub executable: bool,
    /// 0=ordinary, 1=stale version, 2=external preimage drift, 3=cancel before mutation,
    /// 4=completion persistence failure then wrong-binding and correct-binding recovery.
    pub scenario: u8,
}

fn bump(metrics: &mut Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

fn blob() -> BoxedStrategy<Blob> {
    (prop::collection::vec(any::<u8>(), 0..32), any::<bool>())
        .prop_map(|(bytes, executable)| Blob { bytes, executable })
        .boxed()
}

fn action() -> BoxedStrategy<Action> {
    (
        0_u8..4,
        1_u8..=3,
        any::<bool>(),
        prop::collection::vec(any::<u8>(), 0..48),
        any::<bool>(),
        prop_oneof![5 => Just(0_u8), 2 => Just(1), 3 => Just(2), 2 => Just(3), 2 => Just(4)],
    )
        .prop_map(|(slot, count, delete_present, payload, executable, scenario)| Action {
            slot,
            count,
            delete_present,
            payload,
            executable,
            scenario,
        })
        .boxed()
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    (
        prop::collection::vec(prop::option::of(blob()), 4..=4),
        prop::collection::vec(action(), 1..=max_steps.clamp(1, 8)),
    )
        .prop_map(|(initial, actions)| Case { initial, actions })
        .boxed()
}

fn mode(blob: &Blob) -> FileMode {
    if cfg!(unix) && blob.executable { FileMode::Executable } else { FileMode::Regular }
}

fn write_blob(path: &Path, blob: &Blob) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, &blob.bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if blob.executable { 0o755 } else { 0o644 }),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn observe_blob(path: &Path) -> Result<Option<Blob>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("unexpected target shape at {}", path.display()));
    }
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = false;
    Ok(Some(Blob { bytes: fs::read(path).map_err(|error| error.to_string())?, executable }))
}

fn check_tree(root: &Path, expected: &[Option<Blob>]) -> Result<(), String> {
    for (slot, desired) in expected.iter().enumerate() {
        let observed = observe_blob(&root.join(PATHS[slot]))?;
        if observed.as_ref().map(|blob| (&blob.bytes, mode(blob)))
            != desired.as_ref().map(|blob| (&blob.bytes, mode(blob)))
        {
            return Err(format!(
                "file-tree mismatch at {}: expected {desired:?}, observed {observed:?}",
                PATHS[slot]
            ));
        }
    }
    if fs::read(root.join("unrelated.txt")).map_err(|error| error.to_string())? != INSIDE_CANARY {
        return Err("unrelated workspace canary changed".into());
    }
    Ok(())
}

fn transaction_empty(root: &Path) -> Result<bool, String> {
    if !root.exists() {
        return Ok(true);
    }
    Ok(fs::read_dir(root).map_err(|error| error.to_string())?.next().is_none())
}

fn selected_slots(action: &Action) -> Vec<usize> {
    (0..usize::from(action.count))
        .map(|offset| (usize::from(action.slot) + offset) % PATHS.len())
        .collect()
}

fn next_blob(action: &Action, slot: usize, step: usize) -> Blob {
    // Prefix ensures that replacement is observably different even if the generated payload
    // happens to equal the old bytes. The model computes this without a production helper.
    let mut bytes = vec![u8::try_from(step).unwrap_or(255), u8::try_from(slot).unwrap_or(255)];
    bytes.extend(&action.payload);
    Blob { bytes, executable: action.executable }
}

fn build_operations(
    current: &[Option<Blob>],
    action: &Action,
    step: usize,
) -> Result<(Vec<PatchOperation>, Vec<Option<Blob>>), String> {
    let mut next = current.to_vec();
    let mut operations = Vec::new();
    for slot in selected_slots(action) {
        let path = WorkspacePath::new(PATHS[slot]).map_err(|error| error.to_string())?;
        let desired = next_blob(action, slot, step);
        let desired_mode = mode(&desired);
        let operation = match &current[slot] {
            None => {
                next[slot] = Some(desired.clone());
                PatchOperation::create(
                    path,
                    FinalFile::new(desired.bytes, desired_mode, LineEndingPolicy::Preserve)
                        .map_err(|error| error.to_string())?,
                )
            }
            Some(before) if action.delete_present => {
                next[slot] = None;
                PatchOperation::delete(path, Preimage::from_bytes(&before.bytes, mode(before)))
                    .map_err(|error| error.to_string())?
            }
            Some(before) => {
                next[slot] = Some(desired.clone());
                PatchOperation::replace(
                    path,
                    Preimage::from_bytes(&before.bytes, mode(before)),
                    FinalFile::new(desired.bytes, desired_mode, LineEndingPolicy::Preserve)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?
            }
        };
        operations.push(operation);
    }
    // The caller deliberately supplies a noncanonical order on alternate steps.
    if step % 2 == 1 {
        operations.reverse();
    }
    Ok((operations, next))
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.initial.len() != PATHS.len() || case.actions.is_empty() || case.actions.len() > 8 {
        return Err("setup-incomplete: patch case shape".into());
    }
    if case.initial.iter().flatten().any(|blob| blob.bytes.len() > 256)
        || case.actions.iter().any(|action| {
            action.slot >= 4
                || !(1..=4).contains(&action.count)
                || action.scenario > 4
                || action.payload.len() > 256
        })
    {
        return Err("setup-incomplete: patch case bounds".into());
    }
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let workspace = temporary.path().join("workspace");
    let transactions = temporary.path().join("transactions");
    let outside_canary = temporary.path().join("outside-canary");
    fs::create_dir(&workspace).map_err(|error| error.to_string())?;
    fs::write(&outside_canary, OUTSIDE_CANARY).map_err(|error| error.to_string())?;
    fs::write(workspace.join("unrelated.txt"), INSIDE_CANARY).map_err(|error| error.to_string())?;
    let mut current = case.initial.clone();
    for (slot, initial) in current.iter().enumerate() {
        if let Some(blob) = initial {
            write_blob(&workspace.join(PATHS[slot]), blob)?;
        }
    }
    let workspace_id = WorkspaceId::new([73; 16]).map_err(|error| format!("{error:?}"))?;
    let generation = Generation::first();
    let mut revision = RevisionNumber::first();
    let mut metrics = Metrics::new();
    bump(&mut metrics, "cases");
    for (step, action) in case.actions.iter().enumerate() {
        let (operations, next) = build_operations(&current, action, step)?;
        let patch = PatchSet::new(workspace_id, generation, revision, operations)
            .map_err(|error| format!("valid patch setup rejected: {error}"))?;
        if !patch.operations().windows(2).all(|pair| pair[0].path() < pair[1].path()) {
            return Err("patch operations were not canonicalized by target path".into());
        }
        let wrong_revision = revision.checked_next().map_err(|error| format!("{error:?}"))?;
        if action.scenario == 1 {
            let denied = patch.plan(workspace_id, generation, wrong_revision);
            if !matches!(denied, Err(ref error) if error.code() == ErrorCode::StaleWorkspace) {
                return Err(format!("stale version accepted: {denied:?}"));
            }
            bump(&mut metrics, "stale_binding_rejections");
        } else {
            let plan = patch
                .plan(workspace_id, generation, revision)
                .map_err(|error| format!("current version rejected: {error}"))?;
            let binding = RecoveryBinding::new(workspace_id, generation, revision);
            match action.scenario {
                0 => {
                    let mut completions = Vec::new();
                    let applied =
                        apply_patch_with_completion(&workspace, &transactions, &plan, |result| {
                            completions.push(result.map(|applied| applied.identity()));
                            Ok(())
                        })
                        .map_err(|error| format!("valid patch application rejected: {error}"))?;
                    if completions != [Some(plan.identity())]
                        || applied.identity() != plan.identity()
                        || applied.cleanup_pending()
                    {
                        return Err("valid patch completion/identity/cleanup differs".into());
                    }
                    current = next;
                    revision = wrong_revision;
                    bump(&mut metrics, "applied_transactions");
                }
                2 => {
                    let slot = selected_slots(action)[0];
                    let mut changed_bytes =
                        current[slot].as_ref().map_or_else(Vec::new, |before| before.bytes.clone());
                    changed_bytes.push(0xA5);
                    let changed = Blob { bytes: changed_bytes, executable: false };
                    write_blob(&workspace.join(PATHS[slot]), &changed)?;
                    current[slot] = Some(changed);
                    let applied =
                        apply_patch_with_completion(&workspace, &transactions, &plan, |_| {
                            Err(peritus_patch::PatchError::completion_persistence_failure())
                        });
                    if !matches!(applied, Err(ref error) if matches!(error.code(), ErrorCode::PreimageMismatch | ErrorCode::PreimageUnexpected))
                    {
                        return Err(format!("changed preimage was not rejected: {applied:?}"));
                    }
                    bump(&mut metrics, "preimage_rejections");
                }
                3 => {
                    let mut completions = Vec::new();
                    let applied = apply_patch_with_completion_and_cancellation(
                        &workspace,
                        &transactions,
                        &plan,
                        || true,
                        |result| {
                            completions.push(result.map(|applied| applied.identity()));
                            Ok(())
                        },
                    )
                    .map_err(|error| format!("pre-mutation cancellation failed: {error}"))?;
                    if applied.is_some() || completions != [None] {
                        return Err(
                            "pre-mutation cancellation performed or misreported work".into()
                        );
                    }
                    bump(&mut metrics, "pre_mutation_cancellations");
                }
                4 => {
                    let failure =
                        apply_patch_with_completion(&workspace, &transactions, &plan, |_| {
                            Err(peritus_patch::PatchError::completion_persistence_failure())
                        });
                    if !matches!(failure, Err(ref error) if error.code() == ErrorCode::Indeterminate)
                    {
                        return Err(format!(
                            "lost completion did not retain uncertainty: {failure:?}"
                        ));
                    }
                    current = next;
                    let directory = transactions.join(format!("txn-{}", plan.identity().to_hex()));
                    if !directory.is_dir() {
                        return Err("uncertain transaction was not retained".into());
                    }
                    check_tree(&workspace, &current)?;
                    let wrong = RecoveryBinding::new(workspace_id, generation, wrong_revision);
                    let denied = recover_transaction(&workspace, &directory, wrong)
                        .map_err(|error| format!("wrong-binding recovery errored: {error}"))?;
                    if denied.state() != RecoveryState::Indeterminate || !directory.is_dir() {
                        return Err("wrong-binding recovery altered the transaction".into());
                    }
                    check_tree(&workspace, &current)?;
                    let recovered = recover_transaction(&workspace, &directory, binding)
                        .map_err(|error| format!("exact-binding recovery errored: {error}"))?;
                    if recovered.state() != RecoveryState::AlreadyApplied
                        || recovered.identity() != Some(plan.identity())
                        || recovered.cleanup_pending()
                        || directory.exists()
                    {
                        return Err(format!("exact-binding recovery differs: {recovered:?}"));
                    }
                    revision = wrong_revision;
                    bump(&mut metrics, "recovered_transactions");
                }
                _ => return Err("unreachable scenario".into()),
            }
        }
        check_tree(&workspace, &current)?;
        if !transaction_empty(&transactions)? {
            return Err(format!("transaction evidence leaked after step {step}"));
        }
        if fs::read(&outside_canary).map_err(|error| error.to_string())? != OUTSIDE_CANARY {
            return Err("sibling-owned canary changed".into());
        }
        bump(&mut metrics, "history_steps");
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    let empty = vec![None; 4];
    let existing = vec![
        Some(Blob { bytes: b"old".to_vec(), executable: false }),
        None,
        Some(Blob { bytes: vec![0, 0xff, b'\n'], executable: true }),
        None,
    ];
    let action = |scenario, slot, count, delete_present, payload: &[u8]| Action {
        slot,
        count,
        delete_present,
        payload: payload.to_vec(),
        executable: false,
        scenario,
    };
    vec![
        Case { initial: empty.clone(), actions: vec![action(0, 0, 3, false, b"create")] },
        Case { initial: existing.clone(), actions: vec![action(0, 0, 4, false, b"replace")] },
        Case { initial: existing.clone(), actions: vec![action(0, 0, 3, true, b"delete")] },
        Case { initial: existing.clone(), actions: vec![action(1, 0, 2, false, b"stale")] },
        Case { initial: existing.clone(), actions: vec![action(2, 0, 3, false, b"drift")] },
        Case { initial: empty.clone(), actions: vec![action(2, 1, 2, false, b"absent")] },
        Case { initial: existing.clone(), actions: vec![action(3, 0, 3, false, b"cancel")] },
        Case { initial: existing.clone(), actions: vec![action(4, 0, 3, false, b"recover")] },
        Case {
            initial: existing,
            actions: vec![
                action(0, 0, 2, false, b"first"),
                action(2, 0, 2, false, b"drift"),
                action(0, 1, 3, true, b"last"),
            ],
        },
    ]
}

pub fn calibration_case() -> Case {
    Case {
        initial: vec![None; 4],
        actions: vec![Action {
            slot: 0,
            count: 1,
            delete_present: false,
            payload: b"stale-binding".to_vec(),
            executable: false,
            scenario: 1,
        }],
    }
}
