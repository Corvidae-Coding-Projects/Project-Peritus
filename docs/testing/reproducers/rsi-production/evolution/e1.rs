//! Real E1 materialization producer. No fabricated receipt, publication, or approval constructors.

use peritus_artifact_store::{
    ArtifactDigest as StoredDigest, ArtifactStore, EncryptionMetadata, MediaType as StoredMedia,
    StoreConfig, WriteRequest,
};
use peritus_daemon::rsi::evolution::{
    EvolutionOperator, EvolutionTransitionIds, MaterializedHarnessRef,
};
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_evolution::*;
use peritus_git::{CreateWorktree, GitRepository, RepositoryOptions, WorktreeAccess, WorktreeName};
use peritus_harness::{
    aggregate::{HarnessCommand, HarnessCommandKind, HarnessState, decide},
    domain::*,
    durability::{DirectiveClaim, commit_harness_transition},
    materialization::*,
    runtime::*,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_product_runner::{MaterializationAuthorityContext, MaterializationAuthorityRequest};
use peritus_types::*;
use peritus_workspace::{
    SnapshotIdentity, WorkspaceBinding, WorkspaceCondition, WorkspaceGateway, WorkspaceState,
    WritableOpenRequest, WritableWorkspace,
};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn id(label: &str) -> [u8; 16] {
    let d = peritus_codec::sha256(format!("peritus-rsi-production-real-e1:{label}").as_bytes());
    let mut b = [0; 16];
    b.copy_from_slice(&d.as_bytes()[..16]);
    if b == [0; 16] {
        b[15] = 1;
    }
    b
}
fn event(label: &str) -> Result<EventId> {
    EventId::new(id(label)).map_err(|_| "event identity rejected".into())
}
fn command(label: &str) -> Result<CommandId> {
    CommandId::new(id(label)).map_err(|_| "command identity rejected".into())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn require(ok: bool, message: &str) -> Result<()> {
    if !ok {
        return Err(message.into());
    }
    Ok(())
}
fn save(store: &ArtifactStore, bytes: &[u8], label: &str) -> Result<StoredDigest> {
    let d = StoredDigest::from_sha256(peritus_codec::sha256(bytes));
    let mut w = store.begin_write(WriteRequest::new(
        d,
        bytes.len() as u64,
        bytes.len().max(1) as u64,
        StoredMedia::new("application/octet-stream")?,
        EncryptionMetadata::unencrypted(),
        event(label)?,
    ))?;
    w.write_chunk(bytes)?;
    Ok(w.finalize()?.digest())
}
fn policy() -> Result<PromotionPolicy> {
    Ok(PromotionPolicy::new(
        PromotionThresholds::new(1, 0, 0, 500_000, 1_000_000, 30_000_000, 0, 0, 0, true, true)?,
        vec![Objective::PairedCorrectness, Objective::Latency],
        vec![ComponentKind::ToolImplementation],
        false,
        4,
        EvolutionLimits::compiled(),
    )?)
}
fn declaration(
    name: &str,
    kind: ComponentKind,
    bytes: &[u8],
    executable: Option<Sha256Digest>,
) -> Result<ComponentDeclaration> {
    let schema = SchemaVersion::new(1)?;
    Ok(ComponentDeclaration::new(
        ComponentIdentity::new(ComponentId::new(name)?, kind, schema),
        ComponentLocation::new(
            SourcePath::new(format!(".peritus-harness/components/{name}.bin"))?,
            TargetPath::new(format!("harness/{name}.bin"))?,
            MediaType::new("application/octet-stream")?,
        ),
        ComponentIntegrity::new(
            bytes.len() as u64,
            peritus_codec::sha256(bytes),
            executable.map(peritus_harness::domain::ArtifactDigest::new),
        ),
        ComponentOwnership::new(
            Owner::new("real-testing operator")?,
            Provenance::new(
                "actual retained CLI executable and canonical frozen promotion policy",
            )?,
        ),
        ComponentRequirements::new(
            vec![],
            CompatibilityContract::new(SchemaInterval::new(schema, schema)?, vec![], vec![])?,
            AuthoritySet::empty(),
            kind.protection_class(),
        ),
        HarnessLimits::compiled(),
    )?)
}
fn revision(
    store: &ArtifactStore,
    executable: &Path,
    previous: Option<&HarnessRevision>,
    root: &Path,
    label: &str,
) -> Result<(HarnessRevision, Sha256Digest)> {
    let bytes = fs::read(executable)?;
    let exe = save(store, &bytes, &format!("{label}-executable"))?.sha256();
    let metadata = serde_json::to_vec_pretty(
        &serde_json::json!({"program":"Peritus CLI","sha256":hex(exe.as_bytes()),"contract":"Cargo package version 0.0.7; unchanged help and invalid-argument behavior","source_executable":executable}),
    )?;
    let strategy = policy()?.canonical_bytes();
    fs::create_dir_all(root)?;
    fs::write(root.join("cli-contract.json"), &metadata)?;
    fs::write(root.join("evolution-strategy.bin"), &strategy)?;
    save(store, &metadata, &format!("{label}-metadata"))?;
    save(store, &strategy, &format!("{label}-strategy"))?;
    let declarations = vec![
        declaration("cli-contract", ComponentKind::ToolImplementation, &metadata, Some(exe))?,
        declaration("evolution-strategy", ComponentKind::EvolutionStrategy, &strategy, None)?,
    ];
    let contents = declarations
        .iter()
        .zip([metadata.clone(), strategy])
        .map(|(d, b)| VerifiedComponentContent::new(d, b))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let graph = CheckedHarnessGraph::check(
        declarations,
        &GraphEnvironment::new(vec![], vec![])?,
        HarnessLimits::compiled(),
    )?;
    let contents = ComponentContents::new(&graph, contents)?;
    let manifest = ManifestDigest::new(peritus_codec::sha256(&graph.canonical_bytes()));
    let revision = match previous {
        None => HarnessRevision::genesis(
            LineageSeed::new(peritus_codec::sha256(
                b"actual Peritus native RSI executable comparison",
            )),
            manifest,
            graph,
            &contents,
        )?,
        Some(p) => HarnessRevision::successor(p, manifest, graph, &contents)?,
    };
    Ok((revision, exe))
}
fn register(
    journal: &mut SqliteJournal,
    prior: Option<&HarnessState>,
    revision: &HarnessRevision,
    label: &str,
) -> Result<HarnessState> {
    let kind = if prior.is_none() {
        HarnessCommandKind::RegisterGenesis { revision: revision.clone() }
    } else {
        HarnessCommandKind::RegisterSuccessor { revision: revision.clone() }
    };
    let c = HarnessCommand::new(
        command(&format!("{label}-register"))?,
        event(&format!("{label}-registered"))?,
        revision.harness_id(),
        prior.map_or(0, HarnessState::sequence),
        prior.map(HarnessState::last_event_id),
        prior.map_or(Sha256Digest::new([0; 32]), HarnessState::state_digest),
        kind,
    )?;
    let t = decide(prior, &c)?;
    commit_harness_transition(journal, &c, &t)?;
    Ok(t.into_parts().1)
}
fn git(root: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git").args(args).current_dir(root).output()?;
    require(
        out.status.success(),
        &format!("real git operation failed: {}", String::from_utf8_lossy(&out.stderr)),
    )
}
fn materialize(
    journal: &mut SqliteJournal,
    store: &ArtifactStore,
    repository: &GitRepository,
    root: &Path,
    state: &HarnessState,
    revision: &HarnessRevision,
    label: &str,
) -> Result<(MaterializationReceipt, RevisionTuple, u64, HarnessState)> {
    let target = root.join(format!("{label}-workspace"));
    let baseline = repository.resolve_baseline("HEAD")?;
    let worktree = repository.create_worktree(CreateWorktree::new(
        WorktreeName::new(format!("{label}-workspace"))?,
        &target,
        baseline,
        WorktreeAccess::Writable,
    ))?;
    let workspace = WorkspaceId::new(id(&format!("{label}-workspace")))
        .map_err(|_| "workspace identity rejected")?;
    let resource = ResourceId::new(id(&format!("{label}-resource")))
        .map_err(|_| "resource identity rejected")?;
    let environment = EnvironmentId::new(id(&format!("{label}-environment")))
        .map_err(|_| "environment identity rejected")?;
    let actor = ActorId::new(id("actor")).map_err(|_| "actor identity rejected")?;
    let session = SessionId::new(id("session")).map_err(|_| "session identity rejected")?;
    let parent = RunId::new(id("host-admitted-E1-run")).map_err(|_| "run identity rejected")?;
    let snapshot = SnapshotIdentity::new(
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        baseline.commit(),
        baseline.tree(),
    );
    let binding = WorkspaceBinding::new(
        workspace,
        resource,
        environment,
        fs::canonicalize(&target)?,
        baseline.commit(),
        baseline.tree(),
    )?;
    let before = WorkspaceSnapshot::from_c1(&snapshot);
    let plan = MaterializationPlan::build(
        command(&format!("{label}-plan"))?,
        event(&format!("{label}-planned"))?,
        revision,
        ObservedTarget::new(before, vec![])?,
        MaterializationReason::Forward,
        None,
    )?;
    let c = HarnessCommand::new(
        command(&format!("{label}-plan"))?,
        event(&format!("{label}-planned"))?,
        revision.harness_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        HarnessCommandKind::PlanMaterialization { plan: plan.clone() },
    )?;
    let committed = match HarnessRuntime::new(journal, store, store).commit_plan(state, &c)? {
        PlanningOutcome::Committed(c) => c,
        PlanningOutcome::AlreadyMaterialized(_) => {
            return Err("fresh target unexpectedly already materialized".into());
        }
    };
    let actions = AuthorizationActions::new(
        ActionId::new(id(&format!("{label}-patch-action"))).map_err(|_| "patch id rejected")?,
        ActionId::new(id(&format!("{label}-candidate-action")))
            .map_err(|_| "candidate id rejected")?,
    );
    let new_snapshot =
        SnapshotId::new(id(&format!("{label}-snapshot"))).map_err(|_| "snapshot id rejected")?;
    let authority = MaterializationAuthorityContext::new(
        root.join("protected-authority"),
        parent,
        if label == "baseline" { 1 } else { 2 },
    )?
    .authorize(&MaterializationAuthorityRequest::new(
        &committed,
        store,
        actor,
        session,
        resource,
        environment,
        actions,
        new_snapshot,
    ))?;
    let mut gateway = WorkspaceGateway::new(WritableWorkspace::open(WritableOpenRequest::new(
        GitRepository::open(RepositoryOptions::new(repository.identity().repository_root()))?,
        worktree,
        WorkspaceState::new(
            binding,
            Generation::first(),
            RevisionNumber::first(),
            snapshot,
            authority.lease_holder(),
            WorkspaceCondition::Clean,
        )?,
        root.join(format!("{label}-transactions")),
    ))?);
    let patch = authority.patch_request(&committed)?;
    let candidate = authority.candidate_request(&committed)?;
    let message = journal.claim_outbox(1, 10_000)?.ok_or("actual E1 outbox directive absent")?;
    let claim = DirectiveClaim::from_message(&plan, &message)?;
    let actual_start =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    let outcome = HarnessRuntime::new(journal, store, store).execute_claimed(
        committed,
        claim,
        &mut gateway,
        RuntimeAuthorizations::new(&patch, &candidate, actions),
        new_snapshot,
        SettlementIds::new(
            command(&format!("{label}-settle"))?,
            event(&format!("{label}-settled"))?,
        ),
        MaterializationTiming::new(actual_start, actual_start)?,
    )?;
    match outcome {
        RuntimeOutcome::Completed { receipt, state, settlement_batch, .. } => {
            let r = RevisionTuple::new(
                authority.revision().acceptance_spec_id(),
                revision.harness_id(),
                workspace,
                receipt.after().generation(),
                receipt.after().revision(),
                authority.revision().policy_id(),
                authority.revision().provider_profile_id(),
            );
            GoverningHarnessBinding::new(r, revision, &receipt)?;
            require(
                fs::read(target.join("harness/cli-contract.bin"))?
                    == store.read(
                        StoredDigest::from_sha256(
                            revision
                                .graph()
                                .declaration(&ComponentId::new("cli-contract")?)
                                .ok_or("metadata declaration missing")?
                                .content_digest(),
                        ),
                        1024 * 1024,
                    )?,
                "actual C1 installed bytes differ",
            )?;
            if label == "candidate" {
                // Start only; no reviewer completion is asserted by the candidate producer.
                let limits = peritus_review::ReviewLimits::new(
                    4,
                    4,
                    4,
                    64,
                    16,
                    32,
                    32,
                    64,
                    16,
                    64,
                    4096,
                    65536,
                    65536,
                    1024 * 1024,
                    4 * 1024 * 1024,
                )?;
                let review_binding = peritus_review::ReviewBinding::from_contract(
                    authority.contract(),
                    r,
                    revision.digest().digest(),
                    peritus_codec::sha256(receipt.after().tree().as_bytes()),
                    vec![actor],
                    vec![peritus_codec::sha256(include_bytes!("e1.rs"))],
                    limits,
                )?;
                let review_run = RunId::new(id("independent-native-candidate-review"))
                    .map_err(|_| "review run identity rejected")?;
                let review_command = peritus_review::ReviewCommand::new(
                    command("start-native-review")?,
                    event("native-review-started")?,
                    review_run,
                    0,
                    None,
                    Sha256Digest::new([0; 32]),
                    r,
                    peritus_review::ReviewCommandKind::StartRun { binding: review_binding, limits },
                )?;
                let review = peritus_review::start(&review_command)?;
                peritus_review::commit_review_transition(journal, &review_command, &review)?;
                fs::write(
                    root.join("candidate-review-binding.json"),
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "review_run":hex(review_run.as_bytes()), "binding_digest":hex(review.state().binding().digest().as_bytes()),
                        "categories":review.state().binding().required_categories().iter().map(|c|hex(c.digest().as_bytes())).collect::<Vec<_>>(),
                        "phase":"Active", "reviewer_execution_required":true,
                    }))?,
                )?;
            }
            Ok((receipt, r, settlement_batch.last_position(), state))
        }
        RuntimeOutcome::Failed { failure, .. } => {
            Err(format!("actual E1 C1 execution failed: {failure:?}").into())
        }
    }
}
fn tuple(r: RevisionTuple) -> serde_json::Value {
    serde_json::json!({"acceptance_spec_id":hex(r.acceptance_spec_id().as_bytes()),"harness_id":hex(r.harness_id().as_bytes()),"workspace_id":hex(r.workspace_id().as_bytes()),"workspace_generation":r.workspace_generation().get(),"workspace_revision":r.workspace_revision().get(),"policy_id":hex(r.policy_id().as_bytes()),"provider_profile_id":hex(r.provider_profile_id().as_bytes())})
}
fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    require(args.len() == 4, "usage: real-e1-consumer fresh-root baseline-cli candidate-cli")?;
    let root = PathBuf::from(&args[1]);
    require(!root.exists(), "fresh evidence root already exists")?;
    fs::create_dir_all(&root)?;
    let db = root.join("peritus.sqlite3");
    let store_id = StoreId::new(id("store"))?;
    let mut journal = SqliteJournal::open(&db, store_id, SqliteJournalOptions::default())?;
    let store = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 512 * 1024 * 1024)?
            .with_database_path(&db)?,
    )?;
    let repo = root.join("source-git");
    fs::create_dir(&repo)?;
    git(&repo, &["init"])?;
    git(&repo, &["config", "user.name", "Peritus Real Testing"])?;
    git(&repo, &["config", "user.email", "real-testing@localhost"])?;
    fs::write(
        repo.join("README.md"),
        "Disposable real E1/C1 harness materialization workspace.\n",
    )?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "-m", "Create actual disposable C1 baseline"])?;
    let repository = GitRepository::open(RepositoryOptions::new(&repo))?;
    let (base, base_exe) =
        revision(&store, Path::new(&args[2]), None, &root.join("baseline-inputs"), "baseline")?;
    let mut state = register(&mut journal, None, &base, "baseline")?;
    let (base_receipt, base_tuple, base_position, next) =
        materialize(&mut journal, &store, &repository, &root, &state, &base, "baseline")?;
    state = next;
    let (candidate, candidate_exe) = revision(
        &store,
        Path::new(&args[3]),
        Some(&base),
        &root.join("candidate-inputs"),
        "candidate",
    )?;
    state = register(&mut journal, Some(&state), &candidate, "candidate")?;
    let (candidate_receipt, candidate_tuple, candidate_position, _) =
        materialize(&mut journal, &store, &repository, &root, &state, &candidate, "candidate")?;
    let mut evidence = EvidenceStore::open(&db, EvidenceStoreOptions::default())?;
    let evidence_id = EvidenceId::new(id("baseline-materialization-evidence"))
        .map_err(|_| "evidence id rejected")?;
    let project = ProjectId::new(id("project")).map_err(|_| "project id rejected")?;
    let mut operator = EvolutionOperator::new(&mut journal, &store, &mut evidence);
    let pointer = operator.initialize(
        project,
        MaterializedHarnessRef::new(base_tuple, base_receipt.id()),
        &ComponentId::new("evolution-strategy")?,
        policy()?,
        EvolutionLimits::compiled(),
        evidence_id,
        EvolutionTransitionIds::new(
            command("initialize-production")?,
            event("initialized-production")?,
        ),
    )?;
    let result = serde_json::json!({"fixture_only":true,"store_id":hex(store_id.as_bytes()),"project_id":hex(project.as_bytes()),"baseline":{"revision":tuple(base_tuple),"harness_revision_number":base.number().get(),"harness_revision_digest":hex(base.digest().as_bytes()),"receipt_id":hex(base_receipt.id().as_bytes()),"receipt_digest":hex(base_receipt.digest().as_bytes()),"program_sha256":hex(base_exe.as_bytes()),"materialization_position":base_position},"candidate":{"revision":tuple(candidate_tuple),"harness_revision_number":candidate.number().get(),"harness_revision_digest":hex(candidate.digest().as_bytes()),"receipt_id":hex(candidate_receipt.id().as_bytes()),"receipt_digest":hex(candidate_receipt.digest().as_bytes()),"program_sha256":hex(candidate_exe.as_bytes()),"materialization_position":candidate_position},"production_pointer_digest":hex(pointer.state_digest().as_bytes()),"baseline_evidence_id":hex(evidence_id.as_bytes())});
    fs::write(root.join("actual-materializations.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
