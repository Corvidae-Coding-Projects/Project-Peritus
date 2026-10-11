//! Exact production terminal classification and ordered non-success identities.

use crate::{SchedulerTerminal, SchedulerTerminalKind, WorkId, WorkRecord, WorkTerminal};

impl SchedulerTerminalKind {}
impl SchedulerTerminal {}
const fn classify(record: &WorkRecord) -> SchedulerTerminalKind {
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
const fn precedence(kind: SchedulerTerminalKind) -> u8 {
    match kind {
        SchedulerTerminalKind::Completed => 0,
        SchedulerTerminalKind::Cancelled => 1,
        SchedulerTerminalKind::Failed => 2,
        SchedulerTerminalKind::DependencyFailed => 3,
        SchedulerTerminalKind::Exhausted => 4,
        SchedulerTerminalKind::Ambiguous => 5,
    }
}
pub(super) fn summarize(work: &[WorkRecord]) -> (SchedulerTerminalKind, Vec<WorkId>) {
    let mut kind = SchedulerTerminalKind::Completed;
    let mut ids = Vec::new();
    let mut index = 0;
    while index < work.len() {
        let record = &work[index];
        let candidate = classify(record);
        if !matches!(candidate, SchedulerTerminalKind::Completed) {
            if precedence(candidate) > precedence(kind) {
                kind = candidate;
            }
            ids.push(record.spec().id());
        }
        index += 1;
    }
    (kind, ids)
}
