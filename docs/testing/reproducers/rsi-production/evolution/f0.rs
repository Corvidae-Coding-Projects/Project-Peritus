//! Actual owner composition over the disposable native E1/E2/E3 fixture.
use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_daemon::rsi::evolution::*;
use peritus_debugger::{DebuggerJobId, DebuggerPhase};
use peritus_eval::*;
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_evolution::*;
use peritus_harness::domain::{ComponentId, RevisionDigest};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::*;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub(crate) fn id(s: &str) -> [u8; 16] {
    let d = peritus_codec::sha256(format!("peritus-rsi-production-real-e1:{s}").as_bytes());
    let mut b = [0; 16];
    b.copy_from_slice(&d.as_bytes()[..16]);
    if b == [0; 16] {
        b[15] = 1;
    }
    b
}
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
pub(crate) fn parse<const N: usize>(s: &str) -> Result<[u8; N]> {
    if s.len() != N * 2 {
        return Err("wrong hex width".into());
    }
    let mut b = [0; N];
    for (i, v) in b.iter_mut().enumerate() {
        *v = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16)?;
    }
    Ok(b)
}
pub(crate) fn key<const N: usize>(v: &serde_json::Value, k: &str) -> Result<[u8; N]> {
    parse(v[k].as_str().ok_or("fixture identity missing")?)
}
pub(crate) fn ids(s: &str) -> Result<EvolutionTransitionIds> {
    let actual_e3 = std::env::args().nth(3).ok_or("actual E3 identity absent")?;
    let s = format!("{actual_e3}:{s}");
    Ok(EvolutionTransitionIds::new(
        CommandId::new(id(&format!("f0-{s}"))).map_err(|_| "command rejected")?,
        EventId::new(id(&format!("f0-event-{s}"))).map_err(|_| "event rejected")?,
    ))
}
fn event(s: &str) -> Result<EventId> {
    EventId::new(id(s)).map_err(|_| "event rejected".into())
}
pub(crate) fn tuple(v: &serde_json::Value) -> Result<RevisionTuple> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(key(v, "acceptance_spec_id")?).map_err(|_| "acceptance rejected")?,
        HarnessId::new(key(v, "harness_id")?).map_err(|_| "harness rejected")?,
        WorkspaceId::new(key(v, "workspace_id")?).map_err(|_| "workspace rejected")?,
        Generation::new(v["workspace_generation"].as_u64().ok_or("generation absent")?)
            .map_err(|_| "generation rejected")?,
        RevisionNumber::new(v["workspace_revision"].as_u64().ok_or("revision absent")?)
            .map_err(|_| "revision rejected")?,
        PolicyId::new(key(v, "policy_id")?).map_err(|_| "policy rejected")?,
        ProviderProfileId::new(key(v, "provider_profile_id")?).map_err(|_| "provider rejected")?,
    ))
}
pub(crate) fn store(
    root: &Path,
) -> Result<(SqliteJournal, ArtifactStore, EvidenceStore, serde_json::Value)> {
    let facts: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("actual-materializations.json"))?)?;
    let db = root.join("peritus.sqlite3");
    Ok((
        SqliteJournal::open(
            &db,
            StoreId::new(key(&facts, "store_id")?)?,
            SqliteJournalOptions::default(),
        )?,
        ArtifactStore::open(
            StoreConfig::new(root.join("artifacts"), 64 * 1024 * 1024, 512 * 1024 * 1024)?
                .with_database_path(&db)?,
        )?,
        EvidenceStore::open(&db, EvidenceStoreOptions::default())?,
        facts,
    ))
}
fn save(a: &ArtifactStore, bytes: &[u8], label: &str) -> Result<ArtifactDigest> {
    let d = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let mut w = a.begin_write(WriteRequest::new(
        d,
        bytes.len() as u64,
        bytes.len().max(1) as u64,
        MediaType::new("application/json")?,
        EncryptionMetadata::unencrypted(),
        event(label)?,
    ))?;
    w.write_chunk(bytes)?;
    Ok(w.finalize()?.digest())
}
pub(crate) fn source(
    j: &SqliteJournal,
    a: &ArtifactStore,
    c: EvaluationCampaignId,
) -> Result<(EvaluationState, FrozenEvaluationProfile, ValidatedEvaluationReport)> {
    let state = load_evaluation_replay(j, c)?.rebuild()?.ok_or("actual E3 absent")?;
    if state.phase() != EvaluationPhase::Published {
        return Err("actual E3 not published".into());
    }
    let p = FrozenEvaluationProfile::from_canonical_bytes(
        &a.read(state.profile_artifact(), 32 * 1024 * 1024)?,
    )?;
    let plan = EvaluationPlan::build(c, &p)?;
    let mut ledger = RolloutLedger::from_plan(&plan, p.retry().maximum_attempts());
    for spec in plan.specs() {
        let RolloutStatus::Settled(t) = state.rollout(spec.id()).ok_or("rollout absent")?.status()
        else {
            return Err("rollout not settled".into());
        };
        let r = RolloutRecord::from_canonical_bytes(&a.read(t.artifact(), t.artifact_bytes())?)?;
        r.validate_spec(spec)?;
        if r.digest() != t.record_digest() {
            return Err("actual record mismatch".into());
        }
        ledger.record_attempt(r.rollout_id(), r.attempt())?;
        ledger.settle(r)?;
    }
    let report = EvaluationReport::new(
        c,
        p.dataset().digest(),
        p.digest(),
        plan.id(),
        plan.digest(),
        analyze_evaluation(&plan, &p, &ledger)?,
        None,
    )?
    .validate()?;
    let rr = state.report().ok_or("actual E3 report absent")?;
    if a.read(rr.artifact(), rr.size())? != report.bytes() {
        return Err("actual published report mismatch".into());
    }
    Ok((state, p, report))
}
fn text(s: &str) -> Result<BoundedText> {
    Ok(BoundedText::new(s.to_owned(), EvolutionLimits::compiled())?)
}
fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: f0-driver stage|admit|select root actual-E3-campaign-hex".into());
    }
    let root = PathBuf::from(&args[2]);
    let c = EvaluationCampaignId::new(parse(&args[3])?).map_err(|_| "E3 identity rejected")?;
    let (mut journal, artifacts, mut evidence, facts) = store(&root)?;
    let job =
        DebuggerJobId::new(id(&format!("actual-native-diagnostic-job:{}", hex(c.as_bytes()))))?;
    if args[1] == "stage" {
        // The owned host initializes its protected capsule directory even when no process started.
        fs::create_dir_all(root.join("rsi"))?;
        let native = stage_local_debugger_report(
            &mut journal,
            &artifacts,
            &root.join("rsi"),
            c,
            job,
            ids("stage-diagnosis")?,
        )?;
        let state = peritus_debugger::load_debugger_replay(&journal, job)?
            .rebuild()?
            .ok_or("E2 state absent")?;
        if state.phase() != DebuggerPhase::ReportReady {
            return Err("E2 staging not ReportReady".into());
        }
        let out = serde_json::json!({"job":hex(job.as_bytes()),"phase":"ReportReady","selected_actual_events":native.selection.counts().events(),"actual_claims":native.report.report().claims().len(),"report_digest":hex(native.report.digest().as_bytes()),"publication_required":true});
        fs::write(root.join("actual-native-e2-staged.json"), serde_json::to_vec_pretty(&out)?)?;
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    let project = ProjectId::new(key(&facts, "project_id")?).map_err(|_| "project rejected")?;
    let campaign =
        EvolutionCampaignId::new(id(&format!("actual-f0-campaign:{}", hex(c.as_bytes()))))
            .map_err(|_| "F0 identity rejected")?;
    let (state, profile, report) = source(&journal, &artifacts, c)?;
    if args[1] == "admit" {
        let native = local_diagnostic_report(&mut journal, &artifacts, c)?;
        let baseline_tuple = tuple(&facts["baseline"]["revision"])?;
        let candidate_tuple = tuple(&facts["candidate"]["revision"])?;
        let expected_receipt = key::<16>(&facts["candidate"], "receipt_id")?;
        let harness = peritus_harness::load_harness_replay(&journal, baseline_tuple.harness_id())?
            .rebuild()?
            .ok_or("real E1 absent")?;
        let candidate_receipt = harness
            .receipts()
            .values()
            .find(|value| value.id().as_bytes() == &expected_receipt)
            .ok_or("actual candidate receipt absent")?
            .id();
        let baseline = harness
            .history()
            .revision(RevisionDigest::new(Sha256Digest::new(key(
                &facts["baseline"],
                "harness_revision_digest",
            )?)))
            .ok_or("real baseline revision absent")?
            .clone();
        let candidate = harness
            .history()
            .revision(RevisionDigest::new(Sha256Digest::new(key(
                &facts["candidate"],
                "harness_revision_digest",
            )?)))
            .ok_or("real candidate revision absent")?
            .clone();
        let comp = ComponentId::new("cli-contract")?;
        let before = baseline.graph().declaration(&comp).ok_or("before component absent")?;
        let after = candidate.graph().declaration(&comp).ok_or("after component absent")?;
        let diff = serde_json::to_vec_pretty(
            &serde_json::json!({"actual_component":"cli-contract","before":serde_json::from_slice::<serde_json::Value>(&artifacts.read(ArtifactDigest::from_sha256(before.content_digest()),1024*1024)?)?,"after":serde_json::from_slice::<serde_json::Value>(&artifacts.read(ArtifactDigest::from_sha256(after.content_digest()),1024*1024)?)?,"unchanged_protected_policy":hex(baseline.graph().declaration(&ComponentId::new("evolution-strategy")?).ok_or("policy absent")?.content_digest().as_bytes())}),
        )?;
        let diff = save(&artifacts, &diff, "actual-component-diff")?;
        let publication = state.publication().ok_or("E3 publication absent")?;
        let exists = recover_campaign(&journal, campaign)?.state().is_some();
        let mut op = EvolutionOperator::new(&mut journal, &artifacts, &mut evidence);
        let mut current = if exists {
            op.campaign(campaign)?
        } else {
            op.create_campaign(campaign, project, EvolutionLimits::compiled(), ids("create")?)?
        };
        if current.phase() == CampaignPhase::Draft {
            current = op.freeze_campaign(campaign, ids("freeze")?)?;
        }
        if current.baseline_evidence().is_empty() {
            current = op.record_baseline(
                campaign,
                publication.evidence_id(),
                state.report().ok_or("E3 report absent")?.artifact(),
                ids("baseline")?,
            )?;
        }
        if current.diagnoses().is_empty() {
            current = op.submit_diagnosis(
                campaign,
                job,
                &native.report,
                vec![DiagnosisCitation::Claim(
                    native
                        .report
                        .report()
                        .claims()
                        .first()
                        .ok_or("actual cited claim absent")?
                        .id(),
                )],
                ids("diagnosis")?,
            )?;
        }
        let diagnosis =
            current.diagnoses().first().ok_or("admitted actual diagnosis absent")?.clone();
        let predictions = vec![Prediction::new(
            PredictionSubject::Campaign,
            PredictionMetric::PairedEffectLower,
            PredictionDirection::AtLeast,
            MetricValue::SignedMillionths(1),
            text(
                "The compiled candidate corrects the known version output defect under the sealed CLI contract.",
            )?,
            true,
            true,
        )?];
        let manifest = ChangeManifest::new(
            &baseline,
            &candidate,
            text(
                "Replacing the baseline Peritus executable with the actual candidate corrects the compiled CLI version while preserving the sealed command contract.",
            )?,
            vec![text(
                "A metadata-only change would leave the compiled baseline version unchanged.",
            )?],
            vec![diagnosis],
            vec![ComponentDelta::capture(
                before,
                after,
                diff.sha256(),
                CompatibilityEffect::Compatible,
                None,
            )?],
            predictions,
            text(
                "Reject if the paired correctness lower bound is nonpositive, or the sealed CLI evaluator records any regression or safety failure.",
            )?,
            baseline.identity(),
            EvolutionLimits::compiled(),
        )?;
        current = op.admit_variant(
            campaign,
            &[manifest],
            MaterializedHarnessRef::new(candidate_tuple, candidate_receipt),
            None,
            ids("variant")?,
        )?;
        let variant = current.variants().first().ok_or("actual variant absent")?.id();
        if current.evaluations().is_empty() {
            current =
                op.admit_evaluation(campaign, variant, c, &profile, &report, ids("evaluation")?)?;
        }
        let pointer = op.pointer(project)?;
        if pointer.current() != current.baseline() {
            return Err("evaluation changed production prematurely".into());
        }
        let out = serde_json::json!({"campaign":hex(campaign.as_bytes()),"phase":format!("{:?}",current.phase()),"variant":hex(variant.as_bytes()),"baseline_still_active":true,"actual_e2_digest":hex(native.report.digest().as_bytes()),"actual_e3_digest":hex(report.digest().as_bytes())});
        fs::write(root.join("actual-f0-admitted.json"), serde_json::to_vec_pretty(&out)?)?;
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    if args[1] == "select" {
        let mut op = EvolutionOperator::new(&mut journal, &artifacts, &mut evidence);
        let current = op.campaign(campaign)?;
        let variant = current.variants().first().ok_or("actual variant absent")?.id();
        let review_run = RunId::new(id("independent-native-candidate-review"))
            .map_err(|_| "review id rejected")?;
        let current =
            op.complete_selection(campaign, &[(variant, review_run)], ids("selection")?)?;
        let out = serde_json::json!({"campaign":hex(campaign.as_bytes()),"phase":format!("{:?}",current.phase()),"selection":format!("{:?}",current.selection()),"assessment":format!("{:?}",current.assessments()),"production_activation_requires_explicit_approval":true});
        fs::write(root.join("actual-f0-selection.json"), serde_json::to_vec_pretty(&out)?)?;
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    Err("unknown operation".into())
}
