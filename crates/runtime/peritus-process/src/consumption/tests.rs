//! Durable claim, terminal, and artifact-publication regressions.

use std::path::PathBuf;

use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, AttemptId, EnvironmentId, HarnessId, PolicyId, ProcessId,
    ProjectId, ProviderProfileId, ResourceId, RevisionTuple, RunId, SessionId, Sha256Digest,
    TurnId, WorkspaceId,
};
use tempfile::TempDir;

use crate::{
    CancellationReason, ErrorCode, EscalationRecord, ExecutionIdentity, LifecyclePhase,
    OsExitObservation, OutputArtifact, OutputCompleteness, OutputStream, OutputSummary,
    ProbeObservation, ProcessInstant, ProcessProbe, ProcessStore, ProcessTreeIdentity,
    RecoveryDisposition, StopTrigger, StreamAccounting, TerminalDisposition, TerminalRecovery,
    TerminalResult, WorkspaceAccess,
    recovery::manifest::ExecutionManifest,
    registry_storage::{persist_claim, write_manifest},
};

#[test]
fn claim_without_manifest_is_an_explicit_indeterminate_recovery_entry() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let claims = registry.registry().join("claims-v1");
    std::fs::create_dir(&claims).expect("claim directory");
    persist_claim(&claims, &identity, digest(30), digest(31)).expect("persist orphan claim");

    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let report = store.reconcile(&mut NoProbe).expect("reconcile orphan claim");
    assert_eq!(report.entries().len(), 1);
    assert_eq!(report.entries()[0].process_id(), process_id);
    assert_eq!(report.entries()[0].disposition(), RecoveryDisposition::Indeterminate);
    assert!(!report.entries()[0].signal_sent());
    assert!(!report.all_terminal());
}

#[test]
fn quarantined_process_records_remain_visible_after_reopen() {
    let registry = TestRegistry::new();
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let corrupt = store.root().join("claims-v1").join("corrupt.claim");
    std::fs::write(corrupt, b"not a canonical process claim").expect("write corrupt record");
    drop(store);

    let first = ProcessStore::open(registry.registry(), registry.workspace()).expect("quarantine");
    assert_eq!(first.quarantined_records().len(), 1);
    drop(first);

    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.quarantined_records().len(), 1);
    let report = reopened.reconcile(&mut NoProbe).expect("reconcile quarantine");
    assert_eq!(report.quarantined_records(), 1);
    assert!(report.entries().is_empty());
    assert!(!report.all_terminal());
}

#[test]
fn claim_manifest_digest_mismatch_blocks_probe_and_terminal_classification() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    prepare_closed_manifest(&registry, &identity, digest(31));
    let claim = registry
        .registry()
        .join("claims-v1")
        .join(format!("{}.claim", crate::registry_storage::hex(process_id.as_bytes())));
    std::fs::remove_file(claim).expect("remove correlated claim");
    persist_claim(&registry.registry().join("claims-v1"), &identity, digest(30), digest(99))
        .expect("persist mismatched claim");

    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let report = store.reconcile(&mut NoProbe).expect("reconcile mismatch");
    assert_eq!(report.entries().len(), 1);
    assert_eq!(report.entries()[0].process_id(), process_id);
    assert_eq!(report.entries()[0].disposition(), RecoveryDisposition::Indeterminate);
    assert!(!report.all_terminal());
}

#[test]
fn complete_terminal_and_artifact_publication_survive_reopen() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let plan_digest = digest(31);
    prepare_closed_manifest(&registry, &identity, plan_digest);
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let terminal = terminal(process_id, plan_digest);
    store.record_terminal(process_id, &terminal).expect("persist terminal");
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("terminal result"), terminal);

    let stream = terminal.output().streams()[0];
    let published = artifact(stream, 50);
    let complete =
        store.record_artifact_publication(process_id, published, true).expect("publish artifact");
    assert!(complete.artifact_publication_complete());
    assert_eq!(complete.artifacts(), &[published]);
    assert_eq!(
        store.record_artifact_publication(process_id, published, true).expect("repeat artifact"),
        complete
    );

    let conflicting = artifact(stream, 51);
    let error = store
        .record_artifact_publication(process_id, conflicting, true)
        .expect_err("conflicting artifact must fail");
    assert_eq!(error.code(), ErrorCode::Artifact);
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("published terminal"), complete);
}

#[test]
fn zero_retained_publication_completion_is_durable_and_idempotent() {
    let registry = TestRegistry::new();
    let identity = identity();
    let process_id = identity.process_id();
    let plan_digest = digest(61);
    prepare_empty_closed_manifest(&registry, &identity, plan_digest);
    let store = ProcessStore::open(registry.registry(), registry.workspace()).expect("open store");
    let mut terminal = TerminalResult::new(
        process_id,
        plan_digest,
        TerminalDisposition::Exited,
        OsExitObservation::Code(0),
        None,
        EscalationRecord::new(false, false, true),
        Some(ProcessInstant::from_millis(1)),
        ProcessInstant::from_millis(2),
        OutputSummary::new(Vec::new(), 0),
        Vec::new(),
        true,
        true,
        TerminalRecovery::OriginalOwner,
    );
    terminal.mark_artifact_failure();
    store.record_terminal(process_id, &terminal).expect("persist incomplete terminal");

    let completed =
        store.complete_artifact_publication(process_id).expect("complete empty publication");
    assert!(completed.artifact_publication_complete());
    assert!(completed.artifacts().is_empty());
    assert_eq!(
        store.complete_artifact_publication(process_id).expect("repeat empty completion"),
        completed
    );
    let reopened = ProcessStore::open(registry.registry(), registry.workspace()).expect("reopen");
    assert_eq!(reopened.terminal_result(process_id).expect("completed terminal"), completed);
}

fn prepare_closed_manifest(
    registry: &TestRegistry,
    identity: &ExecutionIdentity,
    plan_digest: Sha256Digest,
) {
    let claims = registry.registry().join("claims-v1");
    let manifests = registry.registry().join("manifests-v1");
    std::fs::create_dir(&claims).expect("claim directory");
    std::fs::create_dir(&manifests).expect("manifest directory");
    persist_claim(&claims, identity, digest(30), plan_digest).expect("persist claim");
    let manifest = ExecutionManifest {
        identity: *identity,
        action_digest: digest(30),
        plan_digest,
        sandbox_digest: digest(32),
        backend_digest: digest(33),
        support_digest: digest(34),
        preparation_digest: digest(35),
        access: WorkspaceAccess::ReadOnly,
        lease: None,
        phase: LifecyclePhase::Closed,
        tree: None,
        trigger: Some(StopTrigger::new(7, CancellationReason::OutputLimit)),
        exit: Some(OsExitObservation::Code(0)),
        observed_output: 8,
        retained_output: 7,
        dropped_output: 1,
        tree_quiescent: true,
        support_tasks_joined: true,
        terminal_digest: None,
        terminal: None,
    };
    write_manifest(&manifests, &manifest).expect("persist closed manifest");
}

fn prepare_empty_closed_manifest(
    registry: &TestRegistry,
    identity: &ExecutionIdentity,
    plan_digest: Sha256Digest,
) {
    let claims = registry.registry().join("claims-v1");
    let manifests = registry.registry().join("manifests-v1");
    std::fs::create_dir(&claims).expect("claim directory");
    std::fs::create_dir(&manifests).expect("manifest directory");
    persist_claim(&claims, identity, digest(60), plan_digest).expect("persist claim");
    let manifest = ExecutionManifest {
        identity: *identity,
        action_digest: digest(60),
        plan_digest,
        sandbox_digest: digest(62),
        backend_digest: digest(63),
        support_digest: digest(64),
        preparation_digest: digest(65),
        access: WorkspaceAccess::ReadOnly,
        lease: None,
        phase: LifecyclePhase::Closed,
        tree: None,
        trigger: None,
        exit: Some(OsExitObservation::Code(0)),
        observed_output: 0,
        retained_output: 0,
        dropped_output: 0,
        tree_quiescent: true,
        support_tasks_joined: true,
        terminal_digest: None,
        terminal: None,
    };
    write_manifest(&manifests, &manifest).expect("persist empty closed manifest");
}

fn terminal(process_id: ProcessId, plan_digest: Sha256Digest) -> TerminalResult {
    let stream = StreamAccounting::from_persisted(
        OutputStream::Stdout,
        8,
        7,
        1,
        OutputCompleteness::Truncated,
    )
    .expect("stream accounting");
    TerminalResult::new(
        process_id,
        plan_digest,
        TerminalDisposition::OutputLimit,
        OsExitObservation::Code(0),
        Some(StopTrigger::new(7, CancellationReason::OutputLimit)),
        EscalationRecord::new(true, true, true),
        Some(ProcessInstant::from_millis(1)),
        ProcessInstant::from_millis(2),
        OutputSummary::new(vec![stream], 3),
        Vec::new(),
        true,
        true,
        TerminalRecovery::OriginalOwner,
    )
}

fn artifact(stream: StreamAccounting, seed: u8) -> OutputArtifact {
    OutputArtifact::new(
        stream.stream(),
        digest(seed),
        stream.retained(),
        0,
        stream.retained(),
        stream.completeness(),
    )
}

fn identity() -> ExecutionIdentity {
    let workspace = WorkspaceId::new([8; 16]).expect("workspace");
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([12; 16]).expect("acceptance"),
        HarnessId::new([13; 16]).expect("harness"),
        workspace,
        peritus_types::Generation::first(),
        peritus_types::RevisionNumber::first(),
        PolicyId::new([14; 16]).expect("policy"),
        ProviderProfileId::new([15; 16]).expect("provider"),
    );
    ExecutionIdentity::new(
        ProjectId::new([1; 16]).expect("project"),
        SessionId::new([2; 16]).expect("session"),
        RunId::new([3; 16]).expect("run"),
        AttemptId::new([4; 16]).expect("attempt"),
        TurnId::new([5; 16]).expect("turn"),
        ActionId::new([6; 16]).expect("action"),
        ProcessId::new([7; 16]).expect("process"),
        workspace,
        ResourceId::new([9; 16]).expect("resource"),
        EnvironmentId::new([10; 16]).expect("environment"),
        ActorId::new([11; 16]).expect("actor"),
        revision,
    )
}

const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}

struct TestRegistry {
    root: TempDir,
}

impl TestRegistry {
    fn new() -> Self {
        let root = TempDir::new().expect("temporary registry");
        std::fs::create_dir(root.path().join("registry")).expect("registry root");
        std::fs::create_dir(root.path().join("workspace")).expect("workspace root");
        Self { root }
    }

    fn registry(&self) -> PathBuf {
        self.root.path().join("registry")
    }

    fn workspace(&self) -> PathBuf {
        self.root.path().join("workspace")
    }
}

struct NoProbe;

impl ProcessProbe for NoProbe {
    fn observe(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<ProbeObservation, crate::ProcessError> {
        panic!("orphan claim has no process-tree identity to probe")
    }

    fn terminate(&mut self, _identity: ProcessTreeIdentity) -> Result<(), crate::ProcessError> {
        panic!("orphan claim must never be signalled")
    }
}
