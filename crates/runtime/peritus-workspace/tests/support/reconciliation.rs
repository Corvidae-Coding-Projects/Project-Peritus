use peritus_conformance::{WorkspaceConformanceError, WorkspaceReconciliationDisposition};
use peritus_git::{ReconcileDisposition, ReconcileExpectation};
use peritus_leases::{LeaseHolder, LeaseScope, ReconciliationCorrelation};
use peritus_types::{ActorId, EnvironmentId, Generation, SessionId, Sha256Digest};
use peritus_workspace::{
    ReconciliationInput, RestartDisposition, WorkspaceManifest, classify_restart,
};

use super::{ProductionWorkspaceSubject, event_id, infrastructure, resource_id, workspace_id};

pub(super) fn reconcile(
    subject: &mut ProductionWorkspaceSubject,
    expected_generation: u64,
) -> Result<WorkspaceReconciliationDisposition, WorkspaceConformanceError> {
    let expected_generation = Generation::new(expected_generation).map_err(|_| infrastructure())?;
    let expected = correlation(expected_generation);
    let observed = correlation(subject.generation);
    let git = subject.repository.reconcile(ReconcileExpectation::new(
        &subject.writable,
        subject.baseline.commit(),
        subject.current.tree(),
    ));
    let (complete, clean, detail) = git.map_or_else(
        |_| (false, false, Sha256Digest::new([0; 32])),
        |observation| {
            (
                !matches!(observation.disposition(), ReconcileDisposition::Indeterminate(_)),
                matches!(observation.disposition(), ReconcileDisposition::Clean),
                observation.evidence_digest(),
            )
        },
    );
    let observation = classify_restart(ReconciliationInput::new(
        expected, observed, complete, true, clean, detail,
    ));
    let manifest = WorkspaceManifest::reconciliation(
        workspace_id(),
        subject.generation,
        subject.revision,
        subject.current.tree(),
        observation,
    );
    let finalized =
        manifest.finalize(&subject.artifacts, event_id(9)).map_err(|_| infrastructure())?;
    subject.artifacts.verify(finalized.digest()).map_err(|_| infrastructure())?;
    subject.manifest_finalized = true;
    Ok(match observation.disposition() {
        RestartDisposition::Clean => WorkspaceReconciliationDisposition::Clean,
        RestartDisposition::Dirty => WorkspaceReconciliationDisposition::Dirty,
        RestartDisposition::Fenced => WorkspaceReconciliationDisposition::Fenced,
        RestartDisposition::Indeterminate => WorkspaceReconciliationDisposition::Indeterminate,
    })
}

fn correlation(generation: Generation) -> ReconciliationCorrelation {
    ReconciliationCorrelation::new(
        LeaseScope::new(workspace_id(), resource_id(), environment_id()),
        generation,
        LeaseHolder::new(actor_id(), session_id()),
    )
}

fn environment_id() -> EnvironmentId {
    EnvironmentId::new([3; 16]).expect("environment")
}

fn actor_id() -> ActorId {
    ActorId::new([4; 16]).expect("actor")
}

fn session_id() -> SessionId {
    SessionId::new([5; 16]).expect("session")
}
