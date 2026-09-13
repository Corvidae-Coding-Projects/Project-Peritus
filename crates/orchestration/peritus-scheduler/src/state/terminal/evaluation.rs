//! Exact production terminal classification and ordered non-success identities.

use crate::{SchedulerTerminal, SchedulerTerminalKind, WorkId, WorkRecord, WorkTerminal};
use vstd::prelude::*;

verus! {

impl SchedulerTerminalKind {
    /// Defines the established diagnostic priority independently of the evaluator.
    pub open spec fn spec_precedence(self) -> int {
        match self {
            Self::Completed => 0,
            Self::Cancelled => 1,
            Self::Failed => 2,
            Self::DependencyFailed => 3,
            Self::Exhausted => 4,
            Self::Ambiguous => 5,
        }
    }
}

impl SchedulerTerminal {
    /// Classifies the recorded outcome; absent terminal evidence cannot mean success.
    pub open spec fn work_classification(record: WorkRecord) -> SchedulerTerminalKind {
        match record.spec_terminal() {
            Some(WorkTerminal::Succeeded { .. }) => SchedulerTerminalKind::Completed,
            Some(WorkTerminal::Cancelled) => SchedulerTerminalKind::Cancelled,
            Some(WorkTerminal::DependencyFailed { .. }) => SchedulerTerminalKind::DependencyFailed,
            Some(WorkTerminal::Exhausted { .. }) => SchedulerTerminalKind::Exhausted,
            Some(WorkTerminal::Ambiguous { .. }) => SchedulerTerminalKind::Ambiguous,
            _ => SchedulerTerminalKind::Failed,
        }
    }

    /// Selects precisely records whose retained outcome does not establish success.
    pub open spec fn is_non_success(record: WorkRecord) -> bool {
        !matches!(record.spec_terminal(), Some(WorkTerminal::Succeeded { .. }))
    }

    /// Defines the complete non-success list in input order, without deduplication.
    pub open spec fn non_successful_ids(work: Seq<WorkRecord>) -> Seq<WorkId> {
        work.filter(|record: WorkRecord| Self::is_non_success(record))
            .map_values(|record: WorkRecord| record.spec_definition().spec_id())
    }

    /// Relates a summary to every input record and the highest-priority present outcome.
    pub open spec fn summary_matches(
        work: Seq<WorkRecord>,
        kind: SchedulerTerminalKind,
        ids: Seq<WorkId>,
    ) -> bool {
        &&& ids == Self::non_successful_ids(work)
        &&& forall |index: int| 0 <= index < work.len() ==>
            Self::work_classification(#[trigger] work[index]).spec_precedence()
                <= kind.spec_precedence()
        &&& kind == SchedulerTerminalKind::Completed || exists |index: int|
            0 <= index < work.len()
                && Self::work_classification(#[trigger] work[index]) == kind
        &&& (kind == SchedulerTerminalKind::Completed <==>
            forall |index: int| 0 <= index < work.len() ==>
                !Self::is_non_success(#[trigger] work[index]))
    }
}

const fn classify(record: &WorkRecord) -> (kind: SchedulerTerminalKind)
    ensures kind == SchedulerTerminal::work_classification(*record),
{
    match record.terminal() {
        Some(WorkTerminal::Succeeded { .. }) => SchedulerTerminalKind::Completed,
        Some(WorkTerminal::Failed { .. } | WorkTerminal::Abandoned { .. }) => {
            SchedulerTerminalKind::Failed
        }
        Some(WorkTerminal::DependencyFailed { .. }) => SchedulerTerminalKind::DependencyFailed,
        Some(WorkTerminal::Ambiguous { .. }) => SchedulerTerminalKind::Ambiguous,
        Some(WorkTerminal::Exhausted { .. }) => SchedulerTerminalKind::Exhausted,
        Some(WorkTerminal::Cancelled) => SchedulerTerminalKind::Cancelled,
        None => SchedulerTerminalKind::Failed,
    }
}

const fn precedence(kind: SchedulerTerminalKind) -> (result: u8)
    ensures result as int == kind.spec_precedence(),
{
    match kind {
        SchedulerTerminalKind::Completed => 0,
        SchedulerTerminalKind::Cancelled => 1,
        SchedulerTerminalKind::Failed => 2,
        SchedulerTerminalKind::DependencyFailed => 3,
        SchedulerTerminalKind::Exhausted => 4,
        SchedulerTerminalKind::Ambiguous => 5,
    }
}

proof fn append_summary(
    work: Seq<WorkRecord>,
    prior: SchedulerTerminalKind,
    ids: Seq<WorkId>,
    record: WorkRecord,
    next: SchedulerTerminalKind,
    next_ids: Seq<WorkId>,
)
    requires
        SchedulerTerminal::summary_matches(work, prior, ids),
        next == if SchedulerTerminal::work_classification(record).spec_precedence()
            > prior.spec_precedence() {
            SchedulerTerminal::work_classification(record)
        } else { prior },
        next_ids == if SchedulerTerminal::is_non_success(record) {
            ids.push(record.spec_definition().spec_id())
        } else { ids },
    ensures SchedulerTerminal::summary_matches(work.push(record), next, next_ids),
{
    work.lemma_filter_push(record, |item: WorkRecord| SchedulerTerminal::is_non_success(item));
    assert(SchedulerTerminal::non_successful_ids(work.push(record)) =~= next_ids);
    assert forall |index: int| 0 <= index < work.push(record).len() implies
        SchedulerTerminal::work_classification(#[trigger] work.push(record)[index])
            .spec_precedence() <= next.spec_precedence() by {
        if index < work.len() {
            assert(SchedulerTerminal::work_classification(work[index]).spec_precedence()
                <= prior.spec_precedence());
        }
    }
    if next != SchedulerTerminalKind::Completed {
        if next == SchedulerTerminal::work_classification(record) {
            assert(work.push(record)[work.len() as int] == record);
        } else {
            let witness = choose |index: int| 0 <= index < work.len()
                && SchedulerTerminal::work_classification(#[trigger] work[index]) == prior;
            assert(work.push(record)[witness] == work[witness]);
        }
    }
    assert forall |index: int| 0 <= index < work.push(record).len()
        && next == SchedulerTerminalKind::Completed implies
            !SchedulerTerminal::is_non_success(#[trigger] work.push(record)[index]) by {
    }
    if next != SchedulerTerminalKind::Completed {
        let witness = choose |index: int| 0 <= index < work.push(record).len()
            && SchedulerTerminal::work_classification(#[trigger] work.push(record)[index]) == next;
        assert(SchedulerTerminal::is_non_success(work.push(record)[witness]));
    }
}

pub(super) fn summarize(work: &[WorkRecord]) -> (result: (SchedulerTerminalKind, Vec<WorkId>))
    ensures SchedulerTerminal::summary_matches(work@, result.0, result.1@),
{
    let mut kind = SchedulerTerminalKind::Completed;
    let mut ids = Vec::new();
    let mut index = 0;
    proof {
        reveal(Seq::filter);
        assert(work@.take(0) =~= Seq::<WorkRecord>::empty());
        assert(SchedulerTerminal::non_successful_ids(work@.take(0)) =~= ids@);
        assert(SchedulerTerminal::summary_matches(work@.take(0), kind, ids@));
    }
    while index < work.len()
        invariant
            index <= work.len(),
            SchedulerTerminal::summary_matches(work@.take(index as int), kind, ids@),
        decreases work.len() - index,
    {
        let ghost prefix = work@.take(index as int);
        let ghost prior_kind = kind;
        let ghost prior_ids = ids@;
        let record = &work[index];
        let candidate = classify(record);
        if !matches!(candidate, SchedulerTerminalKind::Completed) {
            if precedence(candidate) > precedence(kind) {
                kind = candidate;
            }
            ids.push(record.spec().id());
        }
        proof {
            append_summary(prefix, prior_kind, prior_ids, *record, kind, ids@);
            assert(prefix.push(*record) =~= work@.take(index as int + 1));
        }
        index += 1;
    }
    proof { assert(work@.take(index as int) =~= work@); }
    (kind, ids)
}

} // verus!
