//! Executable C1 conformance cases.

use super::{
    WorkspaceConformanceSubject, WorkspaceMutationDisposition, WorkspacePatchFixture,
    WorkspaceReconciliationDisposition,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct WorkspaceCase {
    descriptor: CaseDescriptor,
    kind: WorkspaceCaseKind,
}

#[derive(Clone, Copy)]
enum WorkspaceCaseKind {
    AtomicCandidate,
    StaleGeneration,
    WrongResource,
    ReadOnly,
    Rollback,
    RestartReconciliation,
}

impl<S: WorkspaceConformanceSubject> ConformanceCase<S> for WorkspaceCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            match self.kind {
                WorkspaceCaseKind::AtomicCandidate => atomic_candidate(subject),
                WorkspaceCaseKind::StaleGeneration => {
                    rejection(subject, &stale_fixture(), WorkspaceMutationDisposition::Stale)
                }
                WorkspaceCaseKind::WrongResource => rejection(
                    subject,
                    &wrong_resource_fixture(),
                    WorkspaceMutationDisposition::Unauthorized,
                ),
                WorkspaceCaseKind::ReadOnly => read_only(subject),
                WorkspaceCaseKind::Rollback => rollback(subject),
                WorkspaceCaseKind::RestartReconciliation => restart_reconciliation(subject),
            }
        })
    }
}

/// Returns the production C1 workspace conformance suite.
#[must_use]
pub fn workspace_suite<S: WorkspaceConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.workspace"),
            ReportText::literal(
                "C1 Git worktree, atomic patch, authorization, and recovery contract",
            ),
        ),
        vec![
            boxed_case(
                "atomic-candidate",
                "Two-file patch produces one exact candidate",
                WorkspaceCaseKind::AtomicCandidate,
            ),
            boxed_case(
                "read-only",
                "Read-only snapshots reject mutation",
                WorkspaceCaseKind::ReadOnly,
            ),
            boxed_case(
                "restart-reconcile",
                "Restart distinguishes clean and dirty state",
                WorkspaceCaseKind::RestartReconciliation,
            ),
            boxed_case(
                "rollback",
                "Rollback creates a successor and retains history",
                WorkspaceCaseKind::Rollback,
            ),
            boxed_case(
                "stale-generation",
                "Stale generation cannot mutate",
                WorkspaceCaseKind::StaleGeneration,
            ),
            boxed_case(
                "wrong-resource",
                "Wrong resource cannot mutate",
                WorkspaceCaseKind::WrongResource,
            ),
        ],
    )
}

fn boxed_case<S: WorkspaceConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    kind: WorkspaceCaseKind,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(WorkspaceCase {
        descriptor: CaseDescriptor::new(
            CaseId::catalog(match suffix {
                "atomic-candidate" => "peritus.workspace.atomic-candidate",
                "read-only" => "peritus.workspace.read-only",
                "restart-reconcile" => "peritus.workspace.restart-reconcile",
                "rollback" => "peritus.workspace.rollback",
                "stale-generation" => "peritus.workspace.stale-generation",
                _ => "peritus.workspace.wrong-resource",
            }),
            ReportText::literal(summary),
        ),
        kind,
    })
}

fn atomic_candidate<S: WorkspaceConformanceSubject>(subject: &mut S) -> CaseResult {
    let Ok(before) = subject.snapshot() else {
        return failed("001", "initial snapshot failed");
    };
    let fixture = fixture();
    let Ok(result) = subject.apply(&fixture) else {
        return failed("002", "authorized patch failed");
    };
    let after = result.snapshot();
    let exact = result.disposition() == WorkspaceMutationDisposition::Applied
        && after.generation() == fixture.generation()
        && after.revision() == before.revision() + 1
        && after.tree_id() != before.tree_id()
        && after.first_contents() == Some(fixture.first_contents())
        && after.second_contents() == Some(fixture.second_contents())
        && after.user_ref_unchanged()
        && after.manifest_finalized();
    result_case(exact, after.revision(), "003", "candidate observation was not exact")
}

fn rejection<S: WorkspaceConformanceSubject>(
    subject: &mut S,
    fixture: &WorkspacePatchFixture,
    expected: WorkspaceMutationDisposition,
) -> CaseResult {
    let Ok(before) = subject.snapshot() else {
        return failed("004", "snapshot before rejection failed");
    };
    let Ok(result) = subject.apply(fixture) else {
        return failed("005", "typed rejection failed");
    };
    let exact = result.disposition() == expected && result.snapshot() == &before;
    result_case(exact, before.revision(), "006", "rejected request changed workspace state")
}

fn read_only<S: WorkspaceConformanceSubject>(subject: &mut S) -> CaseResult {
    let Ok(before) = subject.snapshot() else {
        return failed("007", "snapshot before read-only check failed");
    };
    let Ok(result) = subject.apply_read_only(&fixture()) else {
        return failed("008", "read-only request failed");
    };
    let exact = result.disposition() == WorkspaceMutationDisposition::ReadOnly
        && result.snapshot() == &before;
    result_case(exact, before.revision(), "009", "read-only surface permitted mutation")
}

fn rollback<S: WorkspaceConformanceSubject>(subject: &mut S) -> CaseResult {
    let Ok(baseline) = subject.snapshot() else {
        return failed("010", "baseline snapshot failed");
    };
    if subject
        .apply(&fixture())
        .map_or(true, |value| value.disposition() != WorkspaceMutationDisposition::Applied)
    {
        return failed("011", "candidate setup failed");
    }
    let Ok(restored) = subject.rollback() else {
        return failed("012", "rollback failed");
    };
    let exact = restored.revision() == baseline.revision() + 2
        && restored.tree_id() == baseline.tree_id()
        && restored.first_contents() == baseline.first_contents()
        && restored.second_contents() == baseline.second_contents()
        && restored.user_ref_unchanged()
        && restored.prior_candidate_retained();
    result_case(exact, restored.revision(), "013", "rollback did not preserve exact history")
}

fn restart_reconciliation<S: WorkspaceConformanceSubject>(subject: &mut S) -> CaseResult {
    let Ok(generation) = subject.snapshot().map(|snapshot| snapshot.generation()) else {
        return failed("014", "restart snapshot failed");
    };
    if subject.restart().is_err() {
        return failed("015", "clean restart failed");
    }
    let Ok(clean) = subject.reconcile(generation) else {
        return failed("016", "clean reconciliation failed");
    };
    if subject.make_dirty().is_err() || subject.restart().is_err() {
        return failed("017", "dirty restart setup failed");
    }
    let Ok(dirty) = subject.reconcile(generation) else {
        return failed("018", "dirty reconciliation failed");
    };
    let stale_generation = generation.checked_add(1).unwrap_or(0);
    let Ok(fenced) = subject.reconcile(stale_generation) else {
        return failed("019", "fenced reconciliation failed");
    };
    if subject.make_indeterminate().is_err() || subject.restart().is_err() {
        return failed("020", "indeterminate restart setup failed");
    }
    let Ok(indeterminate) = subject.reconcile(generation) else {
        return failed("021", "indeterminate reconciliation failed");
    };
    let exact = clean == WorkspaceReconciliationDisposition::Clean
        && dirty == WorkspaceReconciliationDisposition::Dirty
        && fenced == WorkspaceReconciliationDisposition::Fenced
        && indeterminate == WorkspaceReconciliationDisposition::Indeterminate;
    result_case(exact, 0, "022", "restart reconciliation guessed an incorrect state")
}

fn result_case(
    exact: bool,
    revision: u64,
    code: &'static str,
    summary: &'static str,
) -> CaseResult {
    let observations = vec![
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
        Observation::new(ObservationId::catalog("revision"), ObservationValue::Unsigned(revision)),
    ];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(code, summary))
    }
}

const fn fixture() -> WorkspacePatchFixture {
    WorkspacePatchFixture {
        workspace_id: [1; 16],
        resource_id: [2; 16],
        generation: 1,
        revision: 1,
        first_path: "src/lib.rs",
        first_contents: b"pub fn answer() -> u8 { 42 }\n",
        second_path: "tests/answer.rs",
        second_contents: b"#[test]\nfn answer_is_42() { assert_eq!(42, 42); }\n",
    }
}

const fn stale_fixture() -> WorkspacePatchFixture {
    let mut value = fixture();
    value.generation = 2;
    value
}

const fn wrong_resource_fixture() -> WorkspacePatchFixture {
    let mut value = fixture();
    value.resource_id = [3; 16];
    value
}

fn failed(code: &'static str, summary: &'static str) -> CaseResult {
    CaseResult::failed(Vec::new(), assertion(code, summary))
}

fn assertion(code: &'static str, summary: &'static str) -> AssertionFailure {
    let stable = match code {
        "001" => "PERITUS-WORKSPACE-CONFORMANCE-001",
        "002" => "PERITUS-WORKSPACE-CONFORMANCE-002",
        "003" => "PERITUS-WORKSPACE-CONFORMANCE-003",
        "004" => "PERITUS-WORKSPACE-CONFORMANCE-004",
        "005" => "PERITUS-WORKSPACE-CONFORMANCE-005",
        "006" => "PERITUS-WORKSPACE-CONFORMANCE-006",
        "007" => "PERITUS-WORKSPACE-CONFORMANCE-007",
        "008" => "PERITUS-WORKSPACE-CONFORMANCE-008",
        "009" => "PERITUS-WORKSPACE-CONFORMANCE-009",
        "010" => "PERITUS-WORKSPACE-CONFORMANCE-010",
        "011" => "PERITUS-WORKSPACE-CONFORMANCE-011",
        "012" => "PERITUS-WORKSPACE-CONFORMANCE-012",
        "013" => "PERITUS-WORKSPACE-CONFORMANCE-013",
        "014" => "PERITUS-WORKSPACE-CONFORMANCE-014",
        "015" => "PERITUS-WORKSPACE-CONFORMANCE-015",
        "016" => "PERITUS-WORKSPACE-CONFORMANCE-016",
        "017" => "PERITUS-WORKSPACE-CONFORMANCE-017",
        "018" => "PERITUS-WORKSPACE-CONFORMANCE-018",
        "019" => "PERITUS-WORKSPACE-CONFORMANCE-019",
        "020" => "PERITUS-WORKSPACE-CONFORMANCE-020",
        "021" => "PERITUS-WORKSPACE-CONFORMANCE-021",
        _ => "PERITUS-WORKSPACE-CONFORMANCE-022",
    };
    AssertionFailure::new(FailureCode::catalog(stable), ReportText::literal(summary), None, None)
}
