//! Real E1/C0/C1 materialization with independent path/content/mode and ancestry models.

use peritus_artifact_store::{
    ArtifactDigest as StoredDigest, ArtifactStore, EncryptionMetadata, MediaType as StoredMedia,
    StoreConfig, WriteRequest,
};
use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_harness::domain::*;
use peritus_harness::{
    AuthorizationActions, DirectiveClaim, HarnessCommand, HarnessCommandKind, HarnessRuntime,
    HarnessState, MaterializationPlan, MaterializationReason, MaterializationReceipt,
    MaterializationTiming, ObservedFile, ObservedTarget, PlannedFileOperation, PlanningOutcome,
    RuntimeAuthorizations, RuntimeOutcome, SettlementIds, WorkspaceSnapshot,
    commit_harness_transition, decide, load_harness_replay,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_patch::{FileMode, Preimage, WorkspacePath};
use peritus_product_runner::{MaterializationAuthorityContext, MaterializationAuthorityRequest};
use peritus_types::{
    ActionId, ActorId, CommandId, EnvironmentId, EventId, Generation, ResourceId, RevisionNumber,
    RunId, SessionId, Sha256Digest, SnapshotId, WorkspaceId,
};
use peritus_workspace::{
    SnapshotIdentity, WorkspaceBinding, WorkspaceCondition, WorkspaceGateway, WorkspaceState,
    WritableOpenRequest, WritableWorkspace,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Round {
    pub mask: u8,
    pub payload: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    None,
    Unowned,
    DirtyOwned,
    WrongPatchAuthority,
    WrongCandidateAuthority,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphDefect {
    Duplicate,
    MissingDependency,
    Cycle,
    AncestorPath,
    ContentDigest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum Case {
    Materialize { rounds: Vec<Round>, recover_plan: bool, fault: Fault },
    Graph { defect: GraphDefect },
    History { parents: Vec<u8>, source: u8, target: u8, orphan: bool },
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let rounds = proptest::collection::vec(
        (1_u8..=15, any::<u8>()).prop_map(|(mask, payload)| Round { mask, payload }),
        1..=max_steps.clamp(1, 3),
    );
    let materialize=(rounds,any::<bool>(),prop_oneof![4=>Just(Fault::None),1=>Just(Fault::Unowned),
        1=>Just(Fault::DirtyOwned),1=>Just(Fault::WrongPatchAuthority),1=>Just(Fault::WrongCandidateAuthority)])
        .prop_map(|(mut rounds,recover_plan,fault)|{
            if fault==Fault::DirtyOwned&&rounds.len()==1{rounds.push(Round{mask:2,payload:7});}
            Case::Materialize{rounds,recover_plan,fault}
        });
    let graph = prop_oneof![
        Just(GraphDefect::Duplicate),
        Just(GraphDefect::MissingDependency),
        Just(GraphDefect::Cycle),
        Just(GraphDefect::AncestorPath),
        Just(GraphDefect::ContentDigest)
    ]
    .prop_map(|defect| Case::Graph { defect });
    let history = (
        proptest::collection::vec(any::<u8>(), 1..=max_steps.clamp(1, 16)),
        any::<u8>(),
        any::<u8>(),
        any::<bool>(),
    )
        .prop_map(|(parents, source, target, orphan)| Case::History {
            parents,
            source,
            target,
            orphan,
        });
    prop_oneof![5=>materialize,2=>graph,3=>history].boxed()
}
pub fn calibration() -> Case {
    Case::Materialize {
        rounds: vec![Round { mask: 1, payload: 1 }, Round { mask: 2, payload: 2 }],
        recover_plan: true,
        fault: Fault::None,
    }
}
pub fn examples() -> Vec<Case> {
    let mut result = vec![
        calibration(),
        Case::Materialize {
            rounds: vec![Round { mask: 15, payload: 0 }, Round { mask: 5, payload: 255 }],
            recover_plan: false,
            fault: Fault::None,
        },
        Case::History { parents: vec![0, 0, 1, 2], source: 3, target: 1, orphan: true },
    ];
    for fault in [
        Fault::Unowned,
        Fault::DirtyOwned,
        Fault::WrongPatchAuthority,
        Fault::WrongCandidateAuthority,
    ] {
        result.push(Case::Materialize {
            rounds: vec![Round { mask: 1, payload: 1 }, Round { mask: 2, payload: 2 }],
            recover_plan: true,
            fault,
        });
    }
    for defect in [
        GraphDefect::Duplicate,
        GraphDefect::MissingDependency,
        GraphDefect::Cycle,
        GraphDefect::AncestorPath,
        GraphDefect::ContentDigest,
    ] {
        result.push(Case::Graph { defect });
    }
    result
}
fn err(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}
fn id(label: &str) -> [u8; 16] {
    let hash = peritus_codec::sha256(label.as_bytes());
    let mut value = [0; 16];
    value.copy_from_slice(&hash.as_bytes()[..16]);
    value[0] |= 1;
    value
}
fn command(label: &str) -> Result<CommandId, String> {
    CommandId::new(id(label)).map_err(err)
}
fn event(label: &str) -> Result<EventId, String> {
    EventId::new(id(label)).map_err(err)
}
fn paths(slot: u8) -> &'static str {
    match slot {
        0 => "harness/a.txt",
        1 => "harness/b.bin",
        2 => "harness/λ.txt",
        _ => "harness/z file.md",
    }
}
fn content(slot: u8, payload: u8) -> Vec<u8> {
    let mut bytes = format!("slot={slot};payload={payload};λ\r\n").into_bytes();
    bytes.extend([0, payload, 255]);
    bytes
}
fn expected(round: &Round) -> BTreeMap<String, Vec<u8>> {
    (0_u8..4)
        .filter(|slot| round.mask & (1 << slot) != 0)
        .map(|slot| (paths(slot).to_owned(), content(slot, round.payload)))
        .collect()
}
fn declaration(
    index: usize,
    path: &str,
    bytes: &[u8],
    dependencies: Vec<DependencyRequirement>,
) -> Result<ComponentDeclaration, String> {
    let schema = SchemaVersion::new(1).map_err(err)?;
    let kind = ComponentKind::ReferenceBundle;
    ComponentDeclaration::new(
        ComponentIdentity::new(
            ComponentId::new(format!("component-{index}")).map_err(err)?,
            kind,
            schema,
        ),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{index}.bin")).map_err(err)?,
            TargetPath::new(path).map_err(err)?,
            MediaType::new("application/octet-stream").map_err(err)?,
        ),
        ComponentIntegrity::new(bytes.len() as u64, peritus_codec::sha256(bytes), None),
        ComponentOwnership::new(
            Owner::new("praxis").map_err(err)?,
            Provenance::new("disposable generated E1 component").map_err(err)?,
        ),
        ComponentRequirements::new(
            dependencies,
            CompatibilityContract::new(
                SchemaInterval::new(schema, schema).map_err(err)?,
                Vec::new(),
                Vec::new(),
            )
            .map_err(err)?,
            AuthoritySet::empty(),
            kind.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .map_err(err)
}
fn revision(
    files: &BTreeMap<String, Vec<u8>>,
    previous: Option<&HarnessRevision>,
    nonce: u64,
) -> Result<HarnessRevision, String> {
    let declarations = files
        .iter()
        .enumerate()
        .map(|(i, (path, bytes))| declaration(i, path, bytes, Vec::new()))
        .collect::<Result<Vec<_>, _>>()?;
    let contents = declarations
        .iter()
        .zip(files.values())
        .map(|(d, b)| VerifiedComponentContent::new(d, b.clone()).map_err(err))
        .collect::<Result<Vec<_>, _>>()?;
    let graph = CheckedHarnessGraph::check(
        declarations,
        &GraphEnvironment::new(Vec::new(), Vec::new()).map_err(err)?,
        HarnessLimits::compiled(),
    )
    .map_err(err)?;
    let contents = ComponentContents::new(&graph, contents).map_err(err)?;
    let manifest = ManifestDigest::new(peritus_codec::sha256(&nonce.to_be_bytes()));
    match previous {
        None => HarnessRevision::genesis(
            LineageSeed::new(peritus_codec::sha256(b"stage4 harness lineage")),
            manifest,
            graph,
            &contents,
        ),
        Some(parent) => HarnessRevision::successor(parent, manifest, graph, &contents),
    }
    .map_err(err)
}
fn save(store: &ArtifactStore, bytes: &[u8], label: &str) -> Result<(), String> {
    let mut writer = store
        .begin_write(WriteRequest::new(
            StoredDigest::from_sha256(peritus_codec::sha256(bytes)),
            bytes.len() as u64,
            bytes.len().max(1) as u64,
            StoredMedia::new("application/octet-stream").map_err(err)?,
            EncryptionMetadata::unencrypted(),
            event(label)?,
        ))
        .map_err(err)?;
    writer.write_chunk(bytes).map_err(err)?;
    writer.finalize().map_err(err)?;
    Ok(())
}
fn register(
    journal: &mut SqliteJournal,
    prior: Option<&HarnessState>,
    revision: &HarnessRevision,
    label: &str,
) -> Result<HarnessState, String> {
    let kind = if prior.is_none() {
        HarnessCommandKind::RegisterGenesis { revision: revision.clone() }
    } else {
        HarnessCommandKind::RegisterSuccessor { revision: revision.clone() }
    };
    let command = HarnessCommand::new(
        command(&format!("{label}-register"))?,
        event(&format!("{label}-registered"))?,
        revision.harness_id(),
        prior.map_or(0, HarnessState::sequence),
        prior.map(HarnessState::last_event_id),
        prior.map_or(Sha256Digest::new([0; 32]), HarnessState::state_digest),
        kind,
    )
    .map_err(err)?;
    let transition = decide(prior, &command).map_err(err)?;
    commit_harness_transition(journal, &command, &transition).map_err(err)?;
    Ok(transition.into_parts().1)
}
fn git(root: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(err)?;
    if !output.status.success() {
        return Err(format!(
            "disposable Git operation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}
fn observed(
    gateway: &WorkspaceGateway,
    root: &Path,
    wanted: &BTreeMap<String, Vec<u8>>,
    prior: Option<&MaterializationReceipt>,
) -> Result<ObservedTarget, String> {
    let mut paths = wanted.keys().cloned().collect::<std::collections::BTreeSet<_>>();
    if let Some(receipt) = prior {
        paths.extend(receipt.files().iter().map(|f| f.path().as_str().to_owned()));
    }
    let mut files = Vec::new();
    for path in paths {
        if root.join(&path).is_file() {
            let bytes = fs::read(root.join(&path)).map_err(err)?;
            files.push(
                ObservedFile::new(
                    WorkspacePath::new(path).map_err(err)?,
                    Preimage::Present {
                        digest: peritus_codec::sha256(&bytes),
                        size: bytes.len() as u64,
                        mode: FileMode::Regular,
                    },
                )
                .map_err(err)?,
            );
        }
    }
    ObservedTarget::new(WorkspaceSnapshot::from_c1(gateway.state().current_snapshot()), files)
        .map_err(err)
}
fn inspect(root: &Path, expected: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for slot in 0..4 {
        let path = paths(slot);
        match expected.get(path) {
            Some(bytes) => {
                if fs::read(root.join(path)).map_err(err)? != *bytes {
                    return Err(format!("C1 installed wrong bytes at {path}"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if fs::metadata(root.join(path)).map_err(err)?.permissions().mode() & 0o111 != 0
                    {
                        return Err(format!(
                            "C1 materialized executable mode for regular component {path}"
                        ));
                    }
                }
            }
            None => {
                if root.join(path).exists() {
                    return Err(format!("C1 retained a deleted or unexpected component {path}"));
                }
            }
        }
    }
    if fs::read(root.join("README.md")).map_err(err)? != b"untouched baseline\n" {
        return Err("materialization changed unrelated baseline bytes".into());
    }
    Ok(())
}
fn compare_plan(
    plan: &MaterializationPlan,
    old: &BTreeMap<String, Vec<u8>>,
    new: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let expected_paths =
        old.keys().chain(new.keys()).cloned().collect::<std::collections::BTreeSet<_>>();
    if plan.operations().iter().map(|op| op.path().as_str().to_owned()).collect::<Vec<_>>()
        != expected_paths.into_iter().collect::<Vec<_>>()
    {
        return Err("harness plan omitted/duplicated/misordered install/delete path".into());
    }
    for operation in plan.operations() {
        let path = operation.path().as_str();
        match operation {
            PlannedFileOperation::Install {
                preimage, artifact_digest, byte_length, mode, ..
            } => {
                let bytes = new.get(path).ok_or("unexpected install path")?;
                if *artifact_digest != peritus_codec::sha256(bytes)
                    || *byte_length != bytes.len() as u64
                    || *mode != FileMode::Regular
                    || *preimage
                        != old.get(path).map_or(Preimage::Absent, |bytes| Preimage::Present {
                            digest: peritus_codec::sha256(bytes),
                            size: bytes.len() as u64,
                            mode: FileMode::Regular,
                        })
                {
                    return Err(
                        "harness install content/mode/preimage differs from independent path map"
                            .into(),
                    );
                }
            }
            PlannedFileOperation::Delete { preimage, .. } => {
                let bytes = old.get(path).ok_or("unexpected deletion path")?;
                if new.contains_key(path)
                    || *preimage
                        != (Preimage::Present {
                            digest: peritus_codec::sha256(bytes),
                            size: bytes.len() as u64,
                            mode: FileMode::Regular,
                        })
                {
                    return Err("harness deletion differs from owned path map".into());
                }
            }
        }
    }
    if plan.total_bytes() != new.values().map(|b| b.len() as u64).sum::<u64>() {
        return Err("harness plan total-byte projection drift".into());
    }
    Ok(())
}

fn materialize(
    rounds: &[Round],
    recover_plan: bool,
    fault: Fault,
) -> Result<crate::Metrics, String> {
    if rounds.is_empty()
        || rounds.len() > 3
        || rounds.iter().any(|round| round.mask == 0 || round.mask > 15)
        || (fault == Fault::DirtyOwned && rounds.len() < 2)
    {
        return Err("invalid harness materialization fixture".into());
    }
    let temporary =
        tempfile::Builder::new().prefix("peritus-stage4-harness-").tempdir().map_err(err)?;
    let root = temporary.path();
    let database = root.join("journal.sqlite");
    let store_id = StoreId::new(id("store")).map_err(err)?;
    let mut journal =
        SqliteJournal::open(&database, store_id, SqliteJournalOptions::default()).map_err(err)?;
    let store = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 1_048_576, 32 * 1_048_576)
            .map_err(err)?
            .with_database_path(&database)
            .map_err(err)?,
    )
    .map_err(err)?;
    let repository_path = root.join("repository");
    fs::create_dir(&repository_path).map_err(err)?;
    git(&repository_path, &["init"])?;
    git(&repository_path, &["config", "user.name", "Peritus Praxis"])?;
    git(&repository_path, &["config", "user.email", "praxis@localhost"])?;
    git(&repository_path, &["config", "core.autocrlf", "false"])?;
    git(&repository_path, &["config", "core.filemode", "true"])?;
    fs::write(repository_path.join("README.md"), b"untouched baseline\n").map_err(err)?;
    git(&repository_path, &["add", "README.md"])?;
    git(&repository_path, &["commit", "--no-gpg-sign", "-m", "Disposable baseline"])?;
    let repository = GitRepository::open(RepositoryOptions::new(&repository_path)).map_err(err)?;
    let baseline = repository.resolve_baseline("HEAD").map_err(err)?;
    let target = root.join("workspace");
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("workspace".to_owned()).map_err(err)?,
            &target,
            baseline,
            WorktreeAccess::Writable,
        ))
        .map_err(err)?;
    let workspace = WorkspaceId::new(id("workspace")).map_err(err)?;
    let resource = ResourceId::new(id("resource")).map_err(err)?;
    let environment = EnvironmentId::new(id("environment")).map_err(err)?;
    let actor = ActorId::new(id("actor")).map_err(err)?;
    let session = SessionId::new(id("session")).map_err(err)?;
    let parent = RunId::new(id("materialization-parent")).map_err(err)?;
    let binding = WorkspaceBinding::new(
        workspace,
        resource,
        environment,
        fs::canonicalize(&target).map_err(err)?,
        baseline.commit(),
        baseline.tree(),
    )
    .map_err(err)?;
    let snapshot = SnapshotIdentity::new(
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        baseline.commit(),
        baseline.tree(),
    );
    let state = WorkspaceState::new(
        binding,
        Generation::first(),
        RevisionNumber::first(),
        snapshot,
        peritus_leases::LeaseHolder::new(actor, session),
        WorkspaceCondition::Clean,
    )
    .map_err(err)?;
    let mut gateway = WorkspaceGateway::new(
        WritableWorkspace::open(WritableOpenRequest::new(
            repository.clone(),
            worktree,
            state,
            root.join("transactions"),
        ))
        .map_err(err)?,
    );
    let mut old = BTreeMap::new();
    let mut prior_receipt: Option<MaterializationReceipt> = None;
    let mut prior_revision: Option<HarnessRevision> = None;
    let mut state: Option<HarnessState> = None;
    let mut completed = 0;
    let mut rejected = 0;
    let mut reopens = 0;
    let mut deletes = 0;
    let mut partial = 0;
    let mut idempotent = 0;
    for (index, round) in rounds.iter().enumerate() {
        let label = format!("round-{index}");
        let new = expected(round);
        for (path, bytes) in &new {
            save(&store, bytes, &format!("{label}-artifact-{path}"))?;
        }
        let revision = revision(&new, prior_revision.as_ref(), index as u64)?;
        let registered = register(&mut journal, state.as_ref(), &revision, &label)?;
        let fault_here = if fault == Fault::DirtyOwned { index == 1 } else { index == 0 };
        if fault_here && matches!(fault, Fault::Unowned | Fault::DirtyOwned) {
            let path = if fault == Fault::Unowned { new.keys().next() } else { old.keys().next() }
                .ok_or("missing fault target")?;
            if let Some(parent) = target.join(path).parent() {
                fs::create_dir_all(parent).map_err(err)?;
            }
            fs::write(target.join(path), b"external edit\n").map_err(err)?;
            let before = registered.canonical_bytes().map_err(err)?;
            let plan = MaterializationPlan::build(
                command(&format!("{label}-plan"))?,
                event(&format!("{label}-planned"))?,
                &revision,
                observed(&gateway, &target, &new, prior_receipt.as_ref())?,
                MaterializationReason::Forward,
                prior_receipt.as_ref(),
            );
            if plan.is_ok() {
                return Err("harness planner overwrote unowned or externally changed bytes".into());
            }
            if load_harness_replay(&journal, revision.harness_id())
                .map_err(err)?
                .rebuild()
                .map_err(err)?
                .ok_or("registered harness disappeared")?
                .canonical_bytes()
                .map_err(err)?
                != before
            {
                return Err("rejected plan changed durable harness state".into());
            }
            if fs::read(target.join(path)).map_err(err)? != b"external edit\n" {
                return Err("rejected plan modified external bytes".into());
            }
            rejected += 1;
            state = Some(registered);
            prior_revision = Some(revision.clone());
            break;
        }
        let plan = MaterializationPlan::build(
            command(&format!("{label}-plan"))?,
            event(&format!("{label}-planned"))?,
            &revision,
            observed(&gateway, &target, &new, prior_receipt.as_ref())?,
            MaterializationReason::Forward,
            prior_receipt.as_ref(),
        )
        .map_err(err)?;
        compare_plan(&plan, &old, &new)?;
        let plan_command = HarnessCommand::new(
            plan.command_id(),
            plan.causal_event_id(),
            revision.harness_id(),
            registered.sequence(),
            Some(registered.last_event_id()),
            registered.state_digest(),
            HarnessCommandKind::PlanMaterialization { plan: plan.clone() },
        )
        .map_err(err)?;
        let mut committed = match HarnessRuntime::new(&mut journal, &store, &store)
            .commit_plan(&registered, &plan_command)
            .map_err(err)?
        {
            PlanningOutcome::Committed(committed) => committed,
            _ => return Err("fresh harness plan was treated as already materialized".into()),
        };
        inspect(&target, &old)?;
        if recover_plan {
            drop(committed);
            drop(journal);
            journal = SqliteJournal::open(&database, store_id, SqliteJournalOptions::default())
                .map_err(err)?;
            let replay = load_harness_replay(&journal, revision.harness_id()).map_err(err)?;
            let observed = replay
                .rebuild()
                .map_err(err)?
                .ok_or("committed harness state missing after reopen")?;
            if observed.pending().len() != 1 || observed.receipts().len() != index {
                return Err("plan checkpoint/recovery lost exact pending/receipt count".into());
            }
            committed = HarnessRuntime::new(&mut journal, &store, &store)
                .recover_plan(&replay, plan.id())
                .map_err(err)?;
            inspect(&target, &old)?;
            reopens += 1;
        }
        let actions = AuthorizationActions::new(
            ActionId::new(id(&format!("{label}-patch"))).map_err(err)?,
            ActionId::new(id(&format!("{label}-candidate"))).map_err(err)?,
        );
        let snapshot = SnapshotId::new(id(&format!("{label}-snapshot"))).map_err(err)?;
        let authority =
            MaterializationAuthorityContext::new(root.join("authority"), parent, index as u64 + 1)
                .map_err(err)?
                .authorize(&MaterializationAuthorityRequest::new(
                    &committed,
                    &store,
                    actor,
                    session,
                    resource,
                    environment,
                    actions,
                    snapshot,
                ))
                .map_err(err)?;
        let patch = authority.patch_request(&committed).map_err(err)?;
        let candidate = authority.candidate_request(&committed).map_err(err)?;
        let message = journal
            .claim_outbox(index as u64 + 1, 10_000)
            .map_err(err)?
            .ok_or("committed harness outbox directive missing")?;
        let claim = DirectiveClaim::from_message(&plan, &message).map_err(err)?;
        let authorizations = if fault_here && fault == Fault::WrongPatchAuthority {
            RuntimeAuthorizations::new(&candidate, &candidate, actions)
        } else if fault_here && fault == Fault::WrongCandidateAuthority {
            RuntimeAuthorizations::new(&patch, &patch, actions)
        } else {
            RuntimeAuthorizations::new(&patch, &candidate, actions)
        };
        let outcome = HarnessRuntime::new(&mut journal, &store, &store)
            .execute_claimed(
                committed,
                claim,
                &mut gateway,
                authorizations,
                snapshot,
                SettlementIds::new(
                    command(&format!("{label}-settle"))?,
                    event(&format!("{label}-settled"))?,
                ),
                MaterializationTiming::new(index as u64 + 10, index as u64 + 11).map_err(err)?,
            )
            .map_err(err)?;
        match outcome {
            RuntimeOutcome::Completed { receipt, state: next, .. } => {
                if fault_here
                    && matches!(fault, Fault::WrongPatchAuthority | Fault::WrongCandidateAuthority)
                {
                    return Err("wrong C1 authority yielded completed harness receipt".into());
                }
                inspect(&target, &new)?;
                if receipt.files().len() != new.len()
                    || receipt.files().iter().any(|file| {
                        new.get(file.path().as_str()).is_none_or(|bytes| {
                            file.digest() != peritus_codec::sha256(bytes)
                                || file.byte_length() != bytes.len() as u64
                                || file.mode() != FileMode::Regular
                        })
                    })
                {
                    return Err(
                        "settled harness receipt differs from independently read C1 files".into()
                    );
                }
                if !next.pending().is_empty() || next.receipts().len() != index + 1 {
                    return Err(
                        "successful harness settlement retained pending work or lost receipt"
                            .into(),
                    );
                }
                deletes += old.keys().filter(|path| !new.contains_key(*path)).count() as u64;
                old = new;
                prior_receipt = Some(receipt);
                prior_revision = Some(revision.clone());
                state = Some(next);
                completed += 1;
            }
            RuntimeOutcome::Failed { state: next, .. } => {
                if !fault_here
                    || !matches!(fault, Fault::WrongPatchAuthority | Fault::WrongCandidateAuthority)
                {
                    return Err("valid C1 harness materialization failed".into());
                }
                if !next.pending().is_empty()
                    || next.receipts().len() != index
                    || next.failures().is_empty()
                {
                    return Err("failed materialization lost truthful durable failure state".into());
                }
                inspect(
                    &target,
                    if fault == Fault::WrongCandidateAuthority { &new } else { &old },
                )?;
                partial += u64::from(fault == Fault::WrongCandidateAuthority);
                rejected += 1;
                state = Some(next);
                prior_revision = Some(revision.clone());
                break;
            }
        }
        let before =
            state.as_ref().ok_or("settled state missing")?.canonical_bytes().map_err(err)?;
        drop(journal);
        journal = SqliteJournal::open(&database, store_id, SqliteJournalOptions::default())
            .map_err(err)?;
        let replay = load_harness_replay(&journal, revision.harness_id()).map_err(err)?;
        let restored = replay.rebuild().map_err(err)?.ok_or("settled checkpoint missing")?;
        if restored.canonical_bytes().map_err(err)? != before {
            return Err("harness C0 checkpoint/reopen differs after materialization".into());
        }
        state = Some(restored);
        inspect(&target, &old)?;
        reopens += 1;
    }
    if fault == Fault::None {
        let revision = prior_revision.as_ref().ok_or("last harness revision missing")?;
        let prior = prior_receipt.as_ref().ok_or("last harness receipt missing")?;
        let state = state.as_ref().ok_or("last harness state missing")?;
        let plan = MaterializationPlan::build(
            command("idempotent-plan")?,
            event("idempotent-planned")?,
            revision,
            observed(&gateway, &target, &old, Some(prior))?,
            MaterializationReason::Forward,
            Some(prior),
        )
        .map_err(err)?;
        let command = HarnessCommand::new(
            plan.command_id(),
            plan.causal_event_id(),
            revision.harness_id(),
            state.sequence(),
            Some(state.last_event_id()),
            state.state_digest(),
            HarnessCommandKind::PlanMaterialization { plan },
        )
        .map_err(err)?;
        if !matches!(HarnessRuntime::new(&mut journal,&store,&store).commit_plan(state,&command).map_err(err)?,PlanningOutcome::AlreadyMaterialized(receipt)if receipt.id()==prior.id())
        {
            return Err("exact prior harness receipt was rematerialized".into());
        }
        inspect(&target, &old)?;
        idempotent += 1;
    }
    if let (Some(state), Some(revision)) = (state.as_ref(), prior_revision.as_ref()) {
        let restored = load_harness_replay(&journal, revision.harness_id())
            .map_err(err)?
            .rebuild()
            .map_err(err)?
            .ok_or("final harness replay missing")?;
        if restored.canonical_bytes().map_err(err)? != state.canonical_bytes().map_err(err)? {
            return Err("final harness durable state mismatch".into());
        }
    }
    Ok(crate::Metrics::from([
        ("materializations".into(), completed),
        ("rejected_effects".into(), rejected),
        ("checkpoint_reopens".into(), reopens),
        ("deletions".into(), deletes),
        ("partial_effect_failures".into(), partial),
        ("idempotent_receipts".into(), idempotent),
    ]))
}

fn graph(defect: GraphDefect) -> Result<crate::Metrics, String> {
    let schema = SchemaVersion::new(1).map_err(err)?;
    let dependency = |index: usize| -> Result<DependencyRequirement, String> {
        Ok(DependencyRequirement::new(
            ComponentId::new(format!("component-{index}")).map_err(err)?,
            ComponentKind::ReferenceBundle,
            SchemaInterval::new(schema, schema).map_err(err)?,
            None,
        ))
    };
    let bytes = b"checked content";
    let first = declaration(
        0,
        "harness/a.txt",
        bytes,
        if matches!(defect, GraphDefect::MissingDependency) {
            vec![dependency(99)?]
        } else if matches!(defect, GraphDefect::Cycle) {
            vec![dependency(1)?]
        } else {
            Vec::new()
        },
    )?;
    if matches!(defect, GraphDefect::ContentDigest) {
        if VerifiedComponentContent::new(&first, b"different bytes".to_vec()).is_ok() {
            return Err("harness accepted content outside exact digest/length binding".into());
        }
        return Ok(crate::Metrics::from([("graph_rejections".into(), 1)]));
    }
    let declarations = match defect {
        GraphDefect::Duplicate => vec![first.clone(), first],
        GraphDefect::Cycle => {
            vec![first, declaration(1, "harness/b.txt", bytes, vec![dependency(0)?])?]
        }
        GraphDefect::AncestorPath => vec![
            declaration(0, "harness/a", bytes, Vec::new())?,
            declaration(1, "harness/a/b", bytes, Vec::new())?,
        ],
        _ => vec![first],
    };
    if CheckedHarnessGraph::check(
        declarations,
        &GraphEnvironment::new(Vec::new(), Vec::new()).map_err(err)?,
        HarnessLimits::compiled(),
    )
    .is_ok()
    {
        return Err(format!("harness accepted invalid graph {defect:?}"));
    }
    Ok(crate::Metrics::from([("graph_rejections".into(), 1)]))
}
fn history(parents: &[u8], source: u8, target: u8, orphan: bool) -> Result<crate::Metrics, String> {
    if parents.len() > 16 {
        return Err("harness history fixture exceeds sixteen successors".into());
    }
    let files = expected(&Round { mask: 1, payload: 0 });
    let genesis = revision(&files, None, 0)?;
    let mut history =
        HarnessHistory::new(genesis.clone(), HarnessLimits::compiled()).map_err(err)?;
    let mut revisions = vec![genesis];
    let mut model_parents = vec![None];
    for (index, parent) in parents.iter().enumerate() {
        let parent = usize::from(*parent) % revisions.len();
        let child = revision(
            &expected(&Round { mask: 1, payload: index as u8 + 1 }),
            Some(&revisions[parent]),
            index as u64 + 1,
        )?;
        history.append(child.clone()).map_err(err)?;
        revisions.push(child);
        model_parents.push(Some(parent));
    }
    let ancestor = |candidate: usize, source: usize| {
        let mut cursor = model_parents[source];
        while let Some(index) = cursor {
            if index == candidate {
                return true;
            }
            cursor = model_parents[index];
        }
        false
    };
    let mut comparisons = 0;
    for (candidate, revision) in revisions.iter().enumerate() {
        for (source, other) in revisions.iter().enumerate() {
            if history.is_ancestor(revision.digest(), other.digest()) != ancestor(candidate, source)
            {
                return Err(
                    "harness ancestor relation disagrees with independent parent list".into()
                );
            }
            comparisons += 1;
        }
    }
    let source = usize::from(source) % revisions.len();
    let target = usize::from(target) % revisions.len();
    if history.validate_rollback(revisions[source].digest(), revisions[target].digest()).is_ok()
        != ancestor(target, source)
    {
        return Err("harness rollback accepted sibling/self or rejected strict ancestor".into());
    }
    let before = history.canonical_snapshot();
    if history.append(revisions[source].clone()).is_ok() || history.canonical_snapshot() != before {
        return Err("duplicate harness revision changed append-only history".into());
    }
    if orphan {
        let hidden =
            revision(&expected(&Round { mask: 1, payload: 33 }), Some(&revisions[0]), 777)?;
        let child = revision(&expected(&Round { mask: 1, payload: 34 }), Some(&hidden), 778)?;
        if history.append(child).is_ok() || history.canonical_snapshot() != before {
            return Err("orphan harness revision entered history".into());
        }
    }
    let foreign = revision(&expected(&Round { mask: 1, payload: 99 }), None, 999)?;
    if history.append(foreign).is_ok() || history.canonical_snapshot() != before {
        return Err("foreign harness lineage entered history".into());
    }
    let decoded = HarnessHistory::decode_canonical_snapshot(&before, HarnessLimits::compiled())
        .map_err(err)?;
    if decoded.canonical_snapshot() != before || decoded.revision_count() != revisions.len() {
        return Err("harness history codec changed retained branch structure".into());
    }
    let tips: std::collections::BTreeSet<_> =
        history.branch_tips().iter().map(|revision| revision.digest()).collect();
    let expected_tips: std::collections::BTreeSet<_> = revisions
        .iter()
        .enumerate()
        .filter(|(index, _)| !model_parents.contains(&Some(*index)))
        .map(|(_, revision)| revision.digest())
        .collect();
    if tips != expected_tips {
        return Err("harness branch tips omitted or invented a revision".into());
    }
    Ok(crate::Metrics::from([
        ("history_successors".into(), parents.len() as u64),
        ("ancestry_comparisons".into(), comparisons),
        ("history_rejections".into(), 2 + u64::from(orphan)),
    ]))
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    match case {
        Case::Materialize { rounds, recover_plan, fault } => {
            materialize(rounds, *recover_plan, *fault)
        }
        Case::Graph { defect } => graph(*defect),
        Case::History { parents, source, target, orphan } => {
            history(parents, *source, *target, *orphan)
        }
    }
}
