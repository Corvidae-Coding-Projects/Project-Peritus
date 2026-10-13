//! Actual public daemon/evaluation consumer. No unit suite or synthetic execution observations.
use peritus_approval::CredentialRegistrySnapshot;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, CollectionGeneration, EncryptionMetadata, GcPlan, MediaType,
    StoreConfig, WriteRequest,
};
use peritus_daemon::rsi::local::{
    LocalProcessBackend, LocalProcessCatalog, LocalProcessLimitsDecl, LocalProgram,
    LocalSupportFile,
};
use peritus_daemon::{DaemonConfig, DaemonRuntime};
use peritus_eval::*;
use peritus_harness::domain::{ComponentId, HarnessRevisionIdentity, RevisionDigest};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_model_protocol::*;
use peritus_scheduler::{ResourceEntry, ResourceKind, ResourceQuantity, ResourceVector};
use peritus_types::EventId;
use peritus_types::*;
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn id(label: &str) -> [u8; 16] {
    let d = peritus_codec::sha256(format!("peritus.rsi.native-real:{label}").as_bytes());
    let mut b = [0; 16];
    b.copy_from_slice(&d.as_bytes()[..16]);
    b[0] |= 1;
    b
}
fn helper_path() -> Result<PathBuf> {
    std::env::var_os("PERITUS_RSI_HELPER")
        .map(PathBuf::from)
        .ok_or("set PERITUS_RSI_HELPER to the installed Linux sandbox helper".into())
}
fn cgroup_root() -> Result<PathBuf> {
    std::env::var_os("PERITUS_RSI_CGROUP_ROOT")
        .map(PathBuf::from)
        .ok_or("set PERITUS_RSI_CGROUP_ROOT to this operator's delegated cgroup parent".into())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn bytes<const N: usize>(v: &Value, key: &str) -> Result<[u8; N]> {
    let s = v[key].as_str().ok_or("missing hex field")?;
    if s.len() != N * 2 {
        return Err("hex width".into());
    }
    let mut b = [0; N];
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?
    }
    Ok(b)
}
fn num(v: &Value, key: &str) -> Result<u64> {
    v[key].as_u64().ok_or("missing number".into())
}
fn arm(v: &Value) -> Result<HarnessArmBinding> {
    let t = &v["revision"];
    let hid = HarnessId::new(bytes(t, "harness_id")?).map_err(|e| format!("{e:?}"))?;
    let rev = RevisionTuple::new(
        AcceptanceSpecId::new(bytes(t, "acceptance_spec_id")?).map_err(|e| format!("{e:?}"))?,
        hid,
        WorkspaceId::new(bytes(t, "workspace_id")?).map_err(|e| format!("{e:?}"))?,
        Generation::new(num(t, "workspace_generation")?).map_err(|e| format!("{e:?}"))?,
        RevisionNumber::new(num(t, "workspace_revision")?).map_err(|e| format!("{e:?}"))?,
        PolicyId::new(bytes(t, "policy_id")?).map_err(|e| format!("{e:?}"))?,
        ProviderProfileId::new(bytes(t, "provider_profile_id")?).map_err(|e| format!("{e:?}"))?,
    );
    Ok(HarnessArmBinding::new(
        rev,
        HarnessRevisionIdentity::new(
            hid,
            RevisionNumber::new(num(v, "harness_revision_number")?)
                .map_err(|e| format!("{e:?}"))?,
            RevisionDigest::new(Sha256Digest::new(bytes(v, "harness_revision_digest")?)),
        ),
        Sha256Digest::new(bytes(v, "receipt_digest")?),
    ))
}
fn save(store: &ArtifactStore, bytes: &[u8]) -> Result<ArtifactDigest> {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let event = EventId::new(id(&digest.to_hex())).map_err(|e| format!("{e:?}"))?;
    let mut w = store.begin_write(WriteRequest::new(
        digest,
        bytes.len() as u64,
        (bytes.len() as u64).max(1),
        MediaType::new("application/octet-stream")?,
        EncryptionMetadata::unencrypted(),
        event,
    ))?;
    w.write_chunk(bytes)?;
    Ok(w.finalize()?.digest())
}
fn capture_production(root: &Path, suite: &str) -> Result<Value> {
    let fixture: Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let store = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 1024 * 1024 * 1024)?
            .with_database_path(root.join("peritus.sqlite3"))?,
    )?;
    let journal = SqliteJournal::open(
        root.join("peritus.sqlite3"),
        StoreId::new(bytes(&fixture, "store_id")?)?,
        SqliteJournalOptions::default(),
    )?;
    let current = peritus_daemon::rsi::production::current_program(
        &journal,
        &store,
        ProjectId::new(bytes(&fixture, "project_id")?).map_err(|e| format!("{e:?}"))?,
        &ComponentId::new("cli-contract")?,
    )?;
    let binding = current.binding();
    let selected = if current.artifact().sha256()
        == Sha256Digest::new(bytes(&fixture["baseline"], "program_sha256")?)
    {
        "Baseline"
    } else if current.artifact().sha256()
        == Sha256Digest::new(bytes(&fixture["candidate"], "program_sha256")?)
    {
        "Candidate"
    } else {
        return Err("current production executable is outside actual frozen arm catalogue".into());
    };
    let actual = arm(&fixture[if selected == "Baseline" { "baseline" } else { "candidate" }])?;
    if binding.revision() != actual.revision()
        || binding.harness_revision() != actual.harness_revision()
        || binding.materialization_receipt_digest() != actual.receipt_digest()
    {
        return Err(
            "current production executable has a different exact E1 materialization binding".into(),
        );
    }
    let capture = json!({"source":"fresh actual current_program lookup; inert metadata, not execution authority",
        "selected_arm":selected,"artifact":current.artifact().to_hex(),"bytes":current.bytes(),
        "binding_digest":hex(binding.digest().as_bytes()),"harness_revision":binding.harness_revision().number().get(),
        "harness_digest":hex(binding.harness_revision().digest().as_bytes()),
        "materialization_receipt_digest":hex(binding.materialization_receipt_digest().as_bytes())});
    fs::write(
        root.join(format!("native-{suite}-production-capture.json")),
        serde_json::to_vec_pretty(&capture)?,
    )?;
    Ok(capture)
}
fn profile(root: &Path, suite: &str) -> Result<FrozenEvaluationProfile> {
    let file = root.join(format!("native-{suite}-profile.bin"));
    if file.exists() {
        return Ok(FrozenEvaluationProfile::from_canonical_bytes(&fs::read(file)?)?);
    }
    let production = if suite.starts_with("production-") {
        Some(capture_production(root, suite)?)
    } else {
        None
    };
    let material: Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let store = ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 1024 * 1024 * 1024)?
            .with_database_path(root.join("peritus.sqlite3"))?,
    )?;
    let executable = |v: &Value| -> Result<(ArtifactDigest, u64)> {
        let d = ArtifactDigest::from_sha256(Sha256Digest::new(bytes(v, "program_sha256")?));
        let meta = store.verify(d)?;
        Ok((d, meta.size()))
    };
    let (base, base_size) = executable(&material["baseline"])?;
    let (candidate, candidate_size) = executable(&material["candidate"])?;
    let python = fs::read("/usr/bin/python3")?;
    let py = save(&store, &python)?;
    let script = save(&store, include_bytes!("evaluator.py"))?;
    let evaluator = LocalProgram::new(
        py,
        python.len() as u64,
        vec![
            "{{support:evaluator.py}}".into(),
            "{{sealed_task}}".into(),
            "{{candidate_stdout}}".into(),
            "{{candidate_stderr}}".into(),
            "{{candidate_terminal}}".into(),
        ],
        vec![LocalSupportFile::new("evaluator.py".into(), script)?],
    )?;
    let wall = if suite == "timeout" { 1 } else { 5000 };
    let memory = if suite == "resource" { 8 * 1024 * 1024 } else { 256 * 1024 * 1024 };
    let catalog = LocalProcessCatalog::new(
        LocalProgram::new(base, base_size, vec!["{{task_arguments}}".into()], vec![])?,
        LocalProgram::new(candidate, candidate_size, vec!["{{task_arguments}}".into()], vec![])?,
        evaluator,
        LocalProcessLimitsDecl::new(wall, 5000, memory, 16 * 1024 * 1024, 1024 * 1024, 16)?,
    )?;
    save(&store, &catalog.canonical_bytes()?)?;
    let scratch = root.join("workspaces/rsi");
    fs::create_dir_all(&scratch)?;
    let backend =
        LocalProcessBackend::new(PathBuf::from("/usr/bin/bwrap"), helper_path()?, cgroup_root()?)?;
    let rv = ResourceVector::new(
        vec![
            ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::new(1)?),
            ResourceEntry::new(ResourceKind::MEMORY_BYTES, ResourceQuantity::new(memory)?),
            ResourceEntry::new(ResourceKind::PROCESS, ResourceQuantity::new(16)?),
        ],
        3,
    )?;
    let execution = catalog.execution_binding(&backend, &scratch, rv, 30_000_000, 1)?;
    let limits = EvaluationLimits::new(32, 128, 1, 100, 2, 16 * 1024 * 1024)?;
    let mut cases = vec![
        (
            "version",
            vec!["--version"],
            json!({"code":0,"stdout":"peritus 0.0.7\n","stderr_empty":true}),
        ),
        (
            "help",
            vec!["--help"],
            json!({"code":0,"stdout_contains":["USAGE:","--version","runs"],"stderr_empty":true}),
        ),
        (
            "short-help",
            vec!["-h"],
            json!({"code":0,"stdout_contains":["USAGE:","--version","runs"],"stderr_empty":true}),
        ),
        (
            "help-command",
            vec!["help"],
            json!({"code":0,"stdout_contains":["USAGE:","--version","runs"],"stderr_empty":true}),
        ),
    ];
    for (label, arg) in [
        ("literal-public", "{{public_task}}"),
        ("literal-sealed", "{{sealed_task}}"),
        ("literal-support", "{{support:evil}}"),
        ("literal-braces", "{{}}"),
        ("unicode", "🌸ブラウザ"),
    ] {
        cases.push((label, vec![arg], json!({"code":2,"stderr_contains":["unknown command:",arg]})))
    }
    if suite != "complete" {
        cases.truncate(1);
        if suite == "cancel" || suite == "crash" || suite == "gc-active" {
            cases[0].2["delay_seconds"] = json!(3)
        }
    }
    if let Some(capture) = &production {
        cases[0].2["stdout"] = json!(if capture["selected_arm"] == "Baseline" {
            "peritus 0.0.0\n"
        } else {
            "peritus 0.0.7\n"
        });
    }
    let mut tasks = Vec::new();
    for (label, args, expected) in cases {
        let public = serde_json::to_vec(&json!({"case":label,"arguments":args}))?;
        let sealed = serde_json::to_vec(&expected)?;
        tasks.push(DatasetTask::new(
            TaskId::new(id(label))?,
            DatasetPartition::Regression,
            1,
            CandidateTaskInput::new(save(&store, &public)?, public.len() as u64)?,
            SealedEvaluatorInput::new(
                save(&store, &sealed)?,
                sealed.len() as u64,
                catalog.evaluator().verifier_digest()?,
            )?,
            catalog.sandbox_policy_digest()?,
        )?)
    }
    tasks.sort_by_key(|t| t.id());
    let dataset=DatasetManifest::new(DatasetId::new(id(&format!("dataset-{suite}")))?,1,tasks,peritus_codec::sha256(b"Peritus CLI source --version=package0.0.7; Usage unknown command code2; help headings; literal argv preserve"),limits)?;
    let provider = FrozenProviderSnapshot::capture(&ProviderProfile::new(
        ProviderProfileId::new(id("declared-local-provider")).map_err(|e| format!("{e:?}"))?,
        1,
        ProviderName::new("restricted-local-process".into())?,
        ModelName::new("no-model-calls".into())?,
        WireDialect::CompatibleChatCompletions,
        CapabilityMatrix::new(&[], &[])?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(1024, 1024, 1, 1, 1024)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::Unsupported,
    )?)?;
    let model = FrozenModelControls::new(
        1024,
        None,
        None,
        peritus_codec::sha256(
            &production
                .as_ref()
                .map(serde_json::to_vec)
                .transpose()?
                .unwrap_or_else(|| b"actual local CLI public arguments".to_vec()),
        ),
        SeedDeliveryPolicy::RecordedOnly,
        provider,
    )?;
    let p = FrozenEvaluationProfile::new(
        dataset,
        vec![DatasetPartition::Regression],
        arm(&material["baseline"])?,
        arm(&material["candidate"])?,
        provider,
        model,
        execution,
        EvaluationRetryPolicy::new(1, 0, 0, limits)?,
        MetricPolicy::new(vec![1, 2], 100, 950_000, 100_000, true, limits)?,
        InfrastructurePolicy::new(
            InfrastructureTreatment::InvalidateMetric,
            InfrastructureTreatment::CountAsFailure,
            InfrastructureTreatment::ExcludeWithDenominator,
        ),
        if suite == "version" { 4 } else { 2 },
        false,
        limits,
    )?;
    fs::write(file, p.canonical_bytes()?)?;
    Ok(p)
}
fn config(root: &Path) -> Result<DaemonConfig> {
    let fixture: Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let registry = root.join("native-approval-registry.bin");
    let journal = SqliteJournal::open(
        root.join("peritus.sqlite3"),
        StoreId::new(bytes(&fixture, "store_id")?)?,
        SqliteJournalOptions::default(),
    )?;
    let generation = match journal.current_credential_registry() {
        Ok(current) => {
            fs::write(&registry, current.snapshot_payload()?)?;
            current.generation()
        }
        Err(error) if error.kind() == peritus_journal::JournalErrorKind::NotFound => 1,
        Err(error) => return Err(error.into()),
    };
    drop(journal);
    if !registry.exists() {
        fs::write(
            &registry,
            CredentialRegistrySnapshot::new(RevisionNumber::first(), vec![])
                .map_err(|e| format!("{e:?}"))?
                .canonical_bytes()
                .map_err(|e| format!("{e:?}"))?,
        )?
    }
    let native_helper = helper_path()?.display().to_string();
    let native_cgroup = cgroup_root()?.display().to_string();
    let text = format!(
        r#"version=1
store_id="{}"
[paths]
state_root="{}"
artifact_root="{}/artifacts"
evidence_root="{}/evidence"
workspace_root="{}/workspaces"
process_root="{}/processes"
transaction_root="{}/transactions"
backup_root="{}/backups"
[approval_registry]
payload_file="{}"
generation={generation}
[human]
actor_id="{}"
[telemetry]
mode="disabled"
[rsi]
bubblewrap="/usr/bin/bwrap"
linux_helper="{native_helper}"
cgroup_root="{native_cgroup}"
[limits]
authority_queue=32
connection_queue=16
maximum_connections=8
maximum_workers=2
maximum_artifact_bytes=67108864
artifact_quota_bytes=1073741824
shutdown_millis=30000
"#,
        fixture["store_id"].as_str().ok_or("fixture store missing")?,
        root.display(),
        root.display(),
        root.display(),
        root.display(),
        root.display(),
        root.display(),
        root.display(),
        registry.display(),
        hex(&id("operator-actor"))
    );
    let text = if root.join("native-without-rsi.flag").exists() {
        let first = text.find("[rsi]\n").ok_or("owned native section disappeared")?;
        let end =
            text[first..].find("[limits]\n").ok_or("owned limits section disappeared")? + first;
        format!("{}{}", &text[..first], &text[end..])
    } else {
        text
    };
    fs::write(root.join("native-daemon.toml"), &text)?;
    Ok(DaemonConfig::parse(&text)?)
}
fn snapshot(state: &EvaluationState) -> Value {
    let counts = state.counts();
    json!({"phase":format!("{:?}",state.phase()),"counts":format!("{counts:?}"),"counts_complete":counts.complete(),"report":format!("{:?}",state.report()),"publication":format!("{:?}",state.publication()),"failure":format!("{:?}",state.failure()),"rollouts":state.rollouts().map(|(id,p)|json!({"id":hex(id.as_bytes()),"status":format!("{:?}",p.status())})).collect::<Vec<_>>()})
}

fn open_store(root: &Path) -> Result<ArtifactStore> {
    Ok(ArtifactStore::open(
        StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 1024 * 1024 * 1024)?
            .with_database_path(root.join("peritus.sqlite3"))?,
    )?)
}
fn gc_snapshot(store: &ArtifactStore, plan: &GcPlan) -> Value {
    json!({"generation":plan.generation().get(),
        "marked":plan.marked().iter().map(|d|d.to_hex()).collect::<Vec<_>>(),
        "current_roots":store.reference_roots().ok().map(|r|r.journal().iter().chain(r.evidence().iter()).map(|d|d.to_hex()).collect::<Vec<_>>()),
        "actions":plan.actions().iter().map(|a|json!({"digest":a.digest().to_hex(),"action":format!("{a:?}")})).collect::<Vec<_>>()})
}
fn apply_gc(root: &Path, label: &str, plan: &GcPlan, observation: Value) -> Result<()> {
    let mut store = open_store(root)?;
    let before = gc_snapshot(&store, plan);
    let application = store.apply_gc_plan(plan);
    let value = json!({"observation":observation,"before":before,
        "application":format!("{application:?}"),
        "marked_after":store.reference_roots()?.journal().iter().chain(store.reference_roots()?.evidence().iter()).map(|d|d.to_hex()).collect::<Vec<_>>()});
    fs::write(root.join(format!("actual-{label}-gc.json")), serde_json::to_vec_pretty(&value)?)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
// Reproducer only: an explicitly owned disposable fixture tests an unsupported second live
// maintenance writer. Candidate roots are identified from real finalized terminal envelopes.
fn active_gc(root: &Path, campaign: EvaluationCampaignId) -> Result<bool> {
    let receipts = root.join("rsi/rsi-child-receipts");
    let Ok(rows) = fs::read_dir(&receipts) else {
        return Ok(false);
    };
    for row in rows {
        let path = row?.path();
        if !path.file_name().is_some_and(|n| n.to_string_lossy().ends_with("-evaluator.json")) {
            continue;
        }
        let receipt: Value = serde_json::from_slice(&fs::read(&path)?)?;
        if receipt["campaign"] != json!(campaign.as_bytes()) {
            continue;
        }
        let preparation = receipt["preparation"]
            .as_array()
            .ok_or("missing preparation bytes")?
            .iter()
            .map(|b| b.as_u64().map(|v| v as u8).ok_or("bad preparation byte"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let leaf = cgroup_root()?.join(format!("peritus-{}", hex(&preparation[..12])));
        let members = fs::read_to_string(leaf.join("cgroup.procs")).unwrap_or_default();
        if members.trim().is_empty() {
            continue;
        }
        let candidate_path = path.with_file_name(
            path.file_name()
                .ok_or("receipt name")?
                .to_string_lossy()
                .replace("-evaluator.json", "-candidate.json"),
        );
        let candidate: Value = serde_json::from_slice(&fs::read(&candidate_path)?)?;
        let store = open_store(root)?;
        let plan = store.plan_gc(CollectionGeneration::new(1)?)?;
        let mut finalized = None;
        for action in plan.actions() {
            let Ok(content) = store.read(action.digest(), 1024 * 1024) else {
                continue;
            };
            let Ok(envelope) = serde_json::from_slice::<Value>(&content) else {
                continue;
            };
            if envelope["process_id"] == candidate["process"]
                && envelope["output_complete"] == true
                && envelope["publication_complete"] == true
            {
                finalized =
                    Some(json!({"terminal_artifact":action.digest().to_hex(),"envelope":envelope}));
                break;
            }
        }
        // After the correction the terminal is marked, so it may not appear in actions.
        if finalized.is_none() {
            for digest in plan.marked() {
                let Ok(content) = store.read(*digest, 1024 * 1024) else {
                    continue;
                };
                let Ok(envelope) = serde_json::from_slice::<Value>(&content) else {
                    continue;
                };
                if envelope["process_id"] == candidate["process"]
                    && envelope["output_complete"] == true
                    && envelope["publication_complete"] == true
                {
                    finalized =
                        Some(json!({"terminal_artifact":digest.to_hex(),"envelope":envelope}));
                    break;
                }
            }
        }
        if let Some(finalized) = finalized {
            apply_gc(
                root,
                "active-stage",
                &plan,
                json!({"live_evaluator_receipt":path,"actual_cgroup":leaf,"actual_members":members,"candidate_receipt":candidate_path,"candidate_finalized":finalized}),
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}
fn inspect(root: &Path, campaign_hex: &str, diagnostics: bool) -> Result<()> {
    let config_file = if root.join("daemon.toml").exists() {
        root.join("daemon.toml")
    } else {
        root.join("native-daemon.toml")
    };
    let config = DaemonConfig::parse(&fs::read_to_string(config_file)?)?;
    let mut journal = SqliteJournal::open(
        root.join("peritus.sqlite3"),
        config.store_identity()?,
        SqliteJournalOptions::default(),
    )?;
    let store = if diagnostics {
        open_store(root)?
    } else {
        ArtifactStore::open(
            StoreConfig::new(
                root.join("inspection-artifacts"),
                64 * 1024 * 1024,
                1024 * 1024 * 1024,
            )?
            .with_database_path(root.join("inspection-artifact-metadata.sqlite3"))?,
        )?
    };
    let campaign = EvaluationCampaignId::new(bytes(&json!({"campaign":campaign_hex}), "campaign")?)
        .map_err(|e| format!("{e:?}"))?;
    let operator = peritus_daemon::rsi::evaluation::EvaluationOperator::new(&mut journal, &store);
    let state = operator.show(campaign)?;
    let diagnostic =
        operator.failure_diagnostic(campaign)?.map(|b| String::from_utf8_lossy(&b).into_owned());
    let value = json!({"campaign":campaign_hex,"state":snapshot(&state),"diagnostic":diagnostic});
    fs::write(
        root.join(format!("actual-public-campaign-{campaign_hex}.json")),
        serde_json::to_vec_pretty(&value)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn gc_final(root: &Path, suite: &str) -> Result<()> {
    let profile = FrozenEvaluationProfile::from_canonical_bytes(&fs::read(
        root.join(format!("native-{suite}-profile.bin")),
    )?)?;
    let fixture: Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let mut store = open_store(root)?;
    let mut journal = SqliteJournal::open(
        root.join("peritus.sqlite3"),
        StoreId::new(bytes(&fixture, "store_id")?)?,
        SqliteJournalOptions::default(),
    )?;
    let campaign = EvaluationCampaignId::new(id(&format!("campaign-{suite}")))
        .map_err(|e| format!("{e:?}"))?;
    let state = peritus_daemon::rsi::evaluation::EvaluationOperator::new(&mut journal, &store)
        .show(campaign)?;
    if state.phase() != EvaluationPhase::Published {
        return Err(
            "final GC requires actual Published campaign and exclusive closed fixture".into()
        );
    }
    let image = ArtifactDigest::from_sha256(profile.execution().image_digest());
    let catalog = LocalProcessCatalog::from_canonical_bytes(&store.read(image, 1024 * 1024)?)?;
    let mut required = vec![
        image,
        ArtifactDigest::from_sha256(peritus_codec::sha256(&profile.canonical_bytes()?)),
        ArtifactDigest::from_sha256(peritus_codec::sha256(&profile.dataset().canonical_bytes()?)),
    ];
    for task in profile.dataset().tasks() {
        required.push(task.candidate_input().artifact());
        required.push(task.evaluator_input().artifact());
    }
    // Canonical descriptor data is inert; this independent retention oracle reads its actual
    // frozen supplementary roots rather than inventing process authorization.
    let canonical: Value = serde_json::from_slice(&catalog.canonical_bytes()?)?;
    for name in ["baseline", "candidate", "evaluator"] {
        let program = &canonical[name];
        let decode = |v: &Value| -> Result<ArtifactDigest> {
            let rows = v.as_array().ok_or("artifact byte array missing")?;
            if rows.len() != 32 {
                return Err("artifact digest width".into());
            }
            let mut digest = [0; 32];
            for (i, b) in rows.iter().enumerate() {
                digest[i] = u8::try_from(b.as_u64().ok_or("artifact digest byte")?)?;
            }
            Ok(ArtifactDigest::new(digest))
        };
        required.push(decode(&program["artifact"])?);
        for file in program["support_files"].as_array().ok_or("support descriptor list missing")? {
            required.push(decode(&file["artifact"])?);
        }
    }
    let witness = save(&store, b"unreferenced closed-campaign GC witness")?;
    let first = store.plan_gc(CollectionGeneration::new(1)?)?;
    let mut terminals = 0;
    for digest in first.marked() {
        let Ok(content) = store.read(*digest, 1024 * 1024) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&content) else {
            continue;
        };
        if v["process_id"].is_array()
            && v["output_complete"] == true
            && v["publication_complete"] == true
        {
            terminals += 1;
            required.push(*digest);
            for field in ["stdout_artifact", "stderr_artifact"] {
                required.push(ArtifactDigest::new(bytes(&v, field)?));
            }
        }
    }
    required.sort();
    required.dedup();
    for root in &required {
        if !first.marked().contains(root) {
            return Err(format!(
                "actual frozen input/output lacks durable GC root: {}",
                root.to_hex()
            )
            .into());
        }
        store.verify(*root)?;
    }
    let first_application = store.apply_gc_plan(&first)?;
    let second = store.plan_gc(CollectionGeneration::new(2)?)?;
    let second_application = store.apply_gc_plan(&second)?;
    if store.metadata(witness)?.is_some()
        || first_application.quarantined() == 0
        || second_application.deleted() == 0
    {
        return Err(
            "actual final GC did not quarantine then delete the unreferenced witness".into()
        );
    }
    for root in &required {
        store.verify(*root)?;
    }
    if terminals != state.counts().expected * 2 {
        return Err("complete campaign GC oracle did not observe both actual terminal stages for every rollout".into());
    }
    let value = json!({"campaign":hex(campaign.as_bytes()),"actual_terminal_envelopes":terminals,"verified_frozen_and_output_roots":required.iter().map(|d|d.to_hex()).collect::<Vec<_>>(),"unreferenced_witness_deleted":witness.to_hex(),"first_application":format!("{first_application:?}"),"second_application":format!("{second_application:?}")});
    fs::write(root.join("actual-final-input-output-gc.json"), serde_json::to_vec_pretty(&value)?)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
fn partial_cancel(root: &Path) -> Result<()> {
    let p = profile(root, "partial-cancel")?;
    let fixture: Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let store = open_store(root)?;
    let mut journal = SqliteJournal::open(
        root.join("peritus.sqlite3"),
        StoreId::new(bytes(&fixture, "store_id")?)?,
        SqliteJournalOptions::default(),
    )?;
    let campaign =
        EvaluationCampaignId::new(id("campaign-partial-cancel")).map_err(|e| format!("{e:?}"))?;
    let plan = EvaluationPlan::build(campaign, &p)?;
    let dataset_artifact = save(&store, &p.dataset().canonical_bytes()?)?;
    let profile_artifact = save(&store, &p.canonical_bytes()?)?;
    let create = EvaluationCommand::new(
        CommandId::new(id("partial-create-command")).map_err(|e| format!("{e:?}"))?,
        EventId::new(id("partial-create-event")).map_err(|e| format!("{e:?}"))?,
        campaign,
        0,
        None,
        Sha256Digest::new([0; 32]),
        p.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: p.arm(EvaluationArm::Baseline).revision(),
            dataset_digest: p.dataset().digest(),
            dataset_artifact,
            profile_artifact,
        },
    )?;
    let transition = decide(None, &create)?;
    commit_evaluation_transition(&mut journal, &create, &transition)?;
    let state = transition.state();
    let spec = plan.specs().first().ok_or("empty actual plan")?;
    let entries = vec![(
        *spec.id().as_bytes(),
        *spec.work_id().as_bytes(),
        *spec.request_digest().as_bytes(),
    )];
    let artifact = save(
        &store,
        &serde_json::to_vec(&(1u16, *plan.id().as_bytes(), *plan.digest().as_bytes(), entries))?,
    )?;
    // A real interrupted plan prefix contains one inert Planned entry and an uncommitted suffix.
    // No RequestSchedule/StartRollout or child authority is created by this prefix.
    let batch = PlanBatch::new(
        1,
        2,
        artifact,
        vec![PlannedRolloutBinding::new(spec.id(), spec.work_id(), spec.request_digest())],
    )?;
    let command = EvaluationCommand::new(
        CommandId::new(id("partial-plan-command")).map_err(|e| format!("{e:?}"))?,
        EventId::new(id("partial-plan-event")).map_err(|e| format!("{e:?}"))?,
        campaign,
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        EvaluationCommandKind::RecordPlanBatch {
            plan_id: plan.id(),
            plan_digest: plan.digest(),
            batch,
        },
    )?;
    let transition = decide(Some(state), &command)?;
    commit_evaluation_transition(&mut journal, &command, &transition)?;
    let before = snapshot(transition.state());
    let mut operator =
        peritus_daemon::rsi::evaluation::EvaluationOperator::new(&mut journal, &store);
    let cancelled = operator
        .cancel(campaign, peritus_codec::sha256(b"actual partial inert-plan cancellation"))?;
    if cancelled.phase() != EvaluationPhase::Cancelled
        || cancelled.counts().cancelled != 1
        || cancelled.counts().passed != 0
        || cancelled.rollouts().any(|(_, r)| {
            matches!(
                r.status(),
                RolloutStatus::Scheduling
                    | RolloutStatus::Scheduled { .. }
                    | RolloutStatus::Running { .. }
            )
        })
    {
        return Err("actual partial Planned prefix did not cancel without effect authority".into());
    }
    let observation = json!({"actual_plan_total":plan.specs().len(),"committed_prefix":1,"before":before,"after":snapshot(&cancelled),"execution_authority_created":false});
    fs::write(
        root.join("actual-partial-planned-cancellation.json"),
        serde_json::to_vec_pretty(&observation)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&observation)?);
    Ok(())
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: native-consumer owned-e1-root SUITE start|resume|capture; native-consumer owned-root inspect CAMPAIGN_HEX".into());
    }
    let root = PathBuf::from(&args[1]);
    let suite = &args[2];
    if args[3] == "gc-final" {
        return gc_final(&root, suite);
    }
    if suite == "partial-cancel" {
        return partial_cancel(&root);
    }
    if suite == "inspect" {
        return inspect(&root, &args[3], true);
    }
    if suite == "inspect-state" {
        return inspect(&root, &args[3], false);
    }
    if args[3] == "capture" {
        println!("{}", serde_json::to_string_pretty(&capture_production(&root, suite)?)?);
        return Ok(());
    }
    let p = profile(&root, suite)?;
    let campaign = EvaluationCampaignId::new(id(&format!("campaign-{suite}")))
        .map_err(|e| format!("{e:?}"))?;
    let plan = EvaluationPlan::build(campaign, &p)?;
    fs::write(root.join(format!("native-{suite}-plan.json")), serde_json::to_vec_pretty(&plan.specs().iter().map(|spec|json!({"rollout":hex(spec.id().as_bytes()),"task":hex(spec.task_id().as_bytes()),"arm":format!("{:?}",spec.arm())})).collect::<Vec<_>>())?)?;
    if suite == "gc-active" {
        let witness = save(&open_store(&root)?, b"unreferenced actual GC witness")?;
        fs::write(
            root.join("actual-gc-witness.json"),
            serde_json::to_vec_pretty(&json!({"unreferenced_artifact":witness.to_hex()}))?,
        )?;
    }
    let stale_gc = if suite == "gc-stale" {
        Some(open_store(&root)?.plan_gc(CollectionGeneration::new(1)?)?)
    } else {
        None
    };
    let runtime = DaemonRuntime::start(config(&root)?).await?;
    fs::write(
        root.join(format!("native-{suite}-started.json")),
        serde_json::to_vec_pretty(
            &json!({"pid":std::process::id(),"campaign":hex(campaign.as_bytes()),"endpoint":format!("{:?}",runtime.endpoint_address())}),
        )?,
    )?;
    println!("ACTUAL DAEMON STARTED {} {}", std::process::id(), suite);
    if args[3] == "start" {
        runtime.authority().start_evaluation(campaign, p).await?;
        if let Some(plan) = &stale_gc {
            apply_gc(
                &root,
                "stale-plan-after-real-admission",
                plan,
                json!({"campaign":hex(campaign.as_bytes()),"source":"plan before real admission; apply after actual C0 RecordArtifacts retention"}),
            )?;
        }
    }
    let started = Instant::now();
    let mut cancelled = false;
    let mut collected = false;
    let mut previous = String::new();
    let mut terminal = None;
    while started.elapsed() < Duration::from_secs(180) {
        let state = runtime
            .authority()
            .evaluation(campaign)
            .await?
            .ok_or("admitted campaign disappeared")?;
        let value = snapshot(&state);
        fs::write(
            root.join(format!("native-{suite}-latest.json")),
            serde_json::to_vec_pretty(&value)?,
        )?;
        let key = format!("{:?} {:?}", state.phase(), state.counts());
        if key != previous {
            println!("{key}");
            previous = key
        }
        if suite == "gc-active" && !collected && state.phase() == EvaluationPhase::Running {
            collected = active_gc(&root, campaign)?;
        }
        if suite == "cancel" && !cancelled && state.phase() == EvaluationPhase::Running {
            let receipts = root.join("rsi/rsi-child-receipts");
            let evaluator = fs::read_dir(&receipts).is_ok_and(|mut rows| {
                rows.any(|row| {
                    row.is_ok_and(|row| {
                        if !row.file_name().to_string_lossy().ends_with("-evaluator.json") {
                            return false;
                        }
                        fs::read(row.path())
                            .ok()
                            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                            .is_some_and(|receipt| {
                                receipt["campaign"] == json!(campaign.as_bytes())
                            })
                    })
                })
            });
            if evaluator {
                runtime
                    .authority()
                    .cancel_evaluation(
                        campaign,
                        peritus_codec::sha256(b"actual native evaluator cancellation replay"),
                    )
                    .await?;
                cancelled = true;
                println!("DURABLE CANCEL REQUESTED");
            }
        }
        if state.phase().terminal() {
            terminal = Some(state);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let shutdown = runtime.shutdown().await?;
    fs::write(
        root.join(format!("native-{suite}-shutdown.json")),
        serde_json::to_vec_pretty(
            &json!({"disposition":format!("{:?}",shutdown.disposition()),"remaining":format!("{:?}",shutdown.remaining()),"failures":format!("{:?}",shutdown.failures())}),
        )?,
    )?;
    let state = terminal.ok_or("native campaign exceeded real 180 second observation bound")?;
    if suite.starts_with("production-") {
        let capture: Value = serde_json::from_slice(&fs::read(
            root.join(format!("native-{suite}-production-capture.json")),
        )?)?;
        let selected = if capture["selected_arm"] == "Baseline" {
            EvaluationArm::Baseline
        } else {
            EvaluationArm::Candidate
        };
        for spec in plan.specs().iter().filter(|spec| spec.arm() == selected) {
            if !state.rollout(spec.id()).is_some_and(|progress|matches!(progress.status(),RolloutStatus::Settled(record) if record.class()==RolloutTerminalClass::Passed)) {
                return Err("actual selected production arm failed independent C2 output oracle".into());
            }
        }
    }
    if suite == "complete" {
        for spec in plan.specs().iter().filter(|spec| spec.arm() == EvaluationArm::Candidate) {
            if !state.rollout(spec.id()).is_some_and(|progress|matches!(progress.status(),RolloutStatus::Settled(record) if record.class()==RolloutTerminalClass::Passed)) {
                return Err("a genuine candidate CLI case was not independently verified Passed".into());
            }
        }
        if state.counts().infrastructure_failed != 0 || state.counts().ambiguous != 0 {
            return Err(
                "ordinary CLI campaign produced infrastructure or ambiguous outcomes".into()
            );
        }
    }
    if suite == "timeout" || suite == "resource" {
        if state.counts().passed != 0 || state.counts().infrastructure_failed == 0 {
            return Err("supervisor limit case was not retained as infrastructure failure".into());
        }
    }
    if suite == "crash" && state.counts().ambiguous == 0 {
        return Err("abrupt-kill recovery did not preserve any actual interrupted outcome".into());
    }

    let value = snapshot(&state);
    fs::write(
        root.join(format!("native-{suite}-result.json")),
        serde_json::to_vec_pretty(&value)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    match suite.as_str() {
        "cancel" => {
            if state.phase() == EvaluationPhase::Cancelled {
                Ok(())
            } else {
                Err("cancel campaign did not reach Cancelled".into())
            }
        }
        _ if state.phase() != EvaluationPhase::Published => {
            Err("actual campaign failed to publish its retained report".into())
        }
        _ => Ok(()),
    }
}
