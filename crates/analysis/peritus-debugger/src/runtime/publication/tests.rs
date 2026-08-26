use std::time::Duration;

use peritus_artifact_store::{ArtifactStore, Publication, StoreConfig};
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceSource, EvidenceStore, EvidenceStoreOptions,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_types::{
    AcceptanceSpecId, CommandId, EventId, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

use crate::{
    AnalysisCounts, DebuggerCommand, DebuggerCommandKind, DebuggerJobId, DebuggerPhase,
    DebuggerRecoveryDecision, DebuggerReport, DebuggerState, PublicationDirectiveClaim,
    SelectionRecord, TraceSelectionManifest, TransitionIds, commit_debugger_transition, decide,
    decide_recovery, finalize_report_artifact, load_debugger_replay, stage_and_commit_report,
    summarize_health,
};

use super::{publish_claimed_report, report_evidence_id};

#[test]
fn staged_report_and_pre_admitted_evidence_reconcile_to_one_publication() {
    let mut stores = Stores::open();
    let (manifest, report) = validated_report();
    let analyzed = analyzed_state(&mut stores.journal, &manifest);

    let first = finalize_report_artifact(
        &stores.artifacts,
        &report,
        EventId::new(bytes(20)).expect("artifact event"),
    )
    .expect("first finalization");
    assert_eq!(first.publication(), Publication::New);
    let retry = finalize_report_artifact(
        &stores.artifacts,
        &report,
        EventId::new(bytes(20)).expect("same artifact event"),
    )
    .expect("exact artifact retry");
    assert_eq!(retry.publication(), Publication::Existing);
    assert_eq!(retry.artifact_digest(), first.artifact_digest());

    let (staged, ready) = stage_and_commit_report(
        &mut stores.journal,
        &stores.artifacts,
        &analyzed,
        &report,
        ids(21),
    )
    .expect("stage and commit report");
    assert_eq!(staged.publication(), Publication::Existing);
    assert_eq!(ready.state().phase(), DebuggerPhase::ReportReady);
    let report_position = ready.batch().last_position();
    let message = stores
        .journal
        .claim_outbox(1, 20)
        .expect("claim publication query")
        .expect("publication directive");
    let claim = PublicationDirectiveClaim::from_message(&message).expect("checked claim");

    let export = stores.journal.integrity_export().expect("integrity export");
    let evidence_id = report_evidence_id(&report).expect("content-derived evidence identity");
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new("debugger-report").expect("evidence kind"),
        EvidenceSource::new("peritus-debugger").expect("evidence source"),
        revision(),
        report_position,
        report.digest(),
        vec![staged.artifact_digest()],
        Vec::new(),
    )
    .expect("report evidence draft");
    let admitted = stores
        .evidence
        .admit(draft, &export, &stores.artifacts)
        .expect("simulate crash after evidence admission");
    assert_eq!(admitted.id(), evidence_id);
    assert_eq!(
        decide_recovery(ready.state(), true, true, true),
        DebuggerRecoveryDecision::ReconcilePublication,
    );

    let published = publish_claimed_report(
        &mut stores.journal,
        &mut stores.evidence,
        &stores.artifacts,
        ready.state(),
        &report,
        staged,
        report_position,
        claim,
        ids(23),
    )
    .expect("idempotent evidence admission and publication settlement");
    assert_eq!(published.evidence(), &admitted);
    assert_eq!(published.committed().state().phase(), DebuggerPhase::Published);
    assert!(stores.journal.claim_outbox(21, 30).expect("outbox query").is_none());
    assert_eq!(stores.evidence.load(evidence_id).expect("load evidence"), Some(admitted),);
    stores.artifacts.verify(staged.artifact_digest()).expect("artifact remains verified");

    let replay = load_debugger_replay(&stores.journal, analyzed.job_id()).expect("load replay");
    let rebuilt = replay.rebuild().expect("rebuild state").expect("published job");
    assert_eq!(rebuilt, published.committed().state().clone());
    assert_eq!(decide_recovery(&rebuilt, true, true, false), DebuggerRecoveryDecision::Complete,);
}

struct Stores {
    _temporary: tempfile::TempDir,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
    evidence: EvidenceStore,
}

impl Stores {
    fn open() -> Self {
        let temporary = tempfile::tempdir().expect("temporary publication directory");
        let database = temporary.path().join("shared.sqlite3");
        let journal = SqliteJournal::open(
            &database,
            StoreId::new(bytes(1)).expect("store identity"),
            SqliteJournalOptions { busy_timeout: Duration::from_millis(500) },
        )
        .expect("open debugger journal");
        let artifacts = ArtifactStore::open(
            StoreConfig::new(temporary.path().join("artifacts"), 1024 * 1024, 8 * 1024 * 1024)
                .expect("artifact configuration")
                .with_database_path(&database)
                .expect("shared artifact database"),
        )
        .expect("open artifact store");
        let evidence = EvidenceStore::open(&database, EvidenceStoreOptions::default())
            .expect("evidence store");
        Self { _temporary: temporary, journal, artifacts, evidence }
    }
}

fn validated_report() -> (TraceSelectionManifest, crate::ValidatedReport) {
    let manifest = TraceSelectionManifest::testing_empty(digest(10));
    let health =
        summarize_health(&manifest, &[], &[], &[], &[], crate::DebuggerLimits::production())
            .expect("empty diagnostic summary");
    let report = DebuggerReport::new(
        &manifest,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        health,
        Vec::new(),
    )
    .validate(&manifest, crate::DebuggerLimits::production())
    .expect("validated empty diagnostic report");
    (manifest, report)
}

fn analyzed_state(journal: &mut SqliteJournal, manifest: &TraceSelectionManifest) -> DebuggerState {
    let create = DebuggerCommand::new(
        CommandId::new(bytes(11)).expect("command identity"),
        EventId::new(bytes(12)).expect("event identity"),
        DebuggerJobId::new(bytes(13)).expect("job identity"),
        0,
        None,
        digest(0),
        manifest.query_digest(),
        DebuggerCommandKind::CreateJob {
            revision: revision(),
            query_digest: manifest.query_digest(),
            limits_digest: digest(14),
            model_plan_digest: None,
        },
    )
    .expect("create command");
    let created = decide(None, &create).expect("create transition");
    commit_debugger_transition(journal, &create, &created).expect("commit create");

    let selection = SelectionRecord::new(manifest.id(), manifest.digest(), 1, 1)
        .expect("nonempty durable selection accounting");
    let select = next(created.state(), 15, DebuggerCommandKind::RecordSelection { selection });
    let selected = decide(Some(created.state()), &select).expect("selection transition");
    commit_debugger_transition(journal, &select, &selected).expect("commit selection");

    let analyze = next(
        selected.state(),
        17,
        DebuggerCommandKind::RecordDeterministicAnalysis {
            analysis_digest: digest(18),
            counts: AnalysisCounts::new(0, 0, 0),
        },
    );
    let analyzed = decide(Some(selected.state()), &analyze).expect("analysis transition");
    commit_debugger_transition(journal, &analyze, &analyzed).expect("commit analysis");
    analyzed.state().clone()
}

fn next(state: &DebuggerState, seed: u8, kind: DebuggerCommandKind) -> DebuggerCommand {
    DebuggerCommand::new(
        CommandId::new(bytes(seed)).expect("command identity"),
        EventId::new(bytes(seed + 1)).expect("event identity"),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        kind,
    )
    .expect("fenced command")
}

fn ids(seed: u8) -> TransitionIds {
    TransitionIds::new(
        CommandId::new(bytes(seed)).expect("command identity"),
        EventId::new(bytes(seed + 1)).expect("event identity"),
    )
}

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(2)).expect("acceptance identity"),
        HarnessId::new(bytes(3)).expect("harness identity"),
        WorkspaceId::new(bytes(4)).expect("workspace identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(5)).expect("policy identity"),
        ProviderProfileId::new(bytes(6)).expect("provider identity"),
    )
}

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
