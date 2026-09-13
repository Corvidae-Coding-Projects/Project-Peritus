//! Exact finite reduction model for supplied H0-H3 qualification reports.

use crate::{QualificationObservation, QualificationSlice, QualificationVerdict, ReleaseCandidate};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub struct QualificationState {
    pub ready_count: u16,
    pub stale_count: u16,
    pub mismatched_count: u16,
    pub unreviewed_count: u16,
    pub not_ready_count: u16,
    pub conflicting: bool,
    pub first_report: Option<Sha256Digest>,
    pub first_verdict: Option<QualificationVerdict>,
    pub aggregate_digest: Seq<u8>,
}

pub open spec fn initial() -> QualificationState {
    QualificationState {
        ready_count: 0,
        stale_count: 0,
        mismatched_count: 0,
        unreviewed_count: 0,
        not_ready_count: 0,
        conflicting: false,
        first_report: None,
        first_verdict: None,
        aggregate_digest: Seq::new(32, |index: int| 0u8),
    }
}

pub open spec fn step(
    state: QualificationState,
    observation: QualificationObservation,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> QualificationState {
    if observation.spec_slice() != slice {
        state
    } else if observation.spec_binding().spec_is_mismatched(candidate) {
        QualificationState {
            mismatched_count: super::super::saturated_increment(state.mismatched_count),
            ..state
        }
    } else if observation.spec_binding().spec_is_stale_at(candidate, evaluated_at) {
        QualificationState {
            stale_count: super::super::saturated_increment(state.stale_count),
            ..state
        }
    } else if !observation.spec_reviewed() {
        QualificationState {
            unreviewed_count: super::super::saturated_increment(state.unreviewed_count),
            ..state
        }
    } else {
        let report_conflict = match state.first_report {
            Some(previous) => !crate::candidate::digest_matches(
                previous,
                observation.spec_report_digest(),
            ),
            None => false,
        };
        let verdict_conflict = match state.first_verdict {
            Some(previous) => previous != observation.spec_verdict(),
            None => false,
        };
        let first_report = match state.first_report {
            Some(previous) => Some(previous),
            None => Some(observation.spec_report_digest()),
        };
        let first_verdict = match state.first_verdict {
            Some(previous) => Some(previous),
            None => Some(observation.spec_verdict()),
        };
        if observation.spec_verdict() == QualificationVerdict::Ready {
            QualificationState {
                ready_count: super::super::saturated_increment(state.ready_count),
                conflicting: state.conflicting || report_conflict || verdict_conflict,
                first_report,
                first_verdict,
                aggregate_digest: super::super::xor_digest_bytes(
                    state.aggregate_digest,
                    observation.spec_report_digest(),
                ),
                ..state
            }
        } else {
            QualificationState {
                not_ready_count: super::super::saturated_increment(state.not_ready_count),
                conflicting: state.conflicting || report_conflict || verdict_conflict,
                first_report,
                first_verdict,
                ..state
            }
        }
    }
}

pub open spec fn through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> QualificationState
    decreases end,
{
    if end == 0 {
        initial()
    } else {
        step(
            through(values, slice, candidate, evaluated_at, (end - 1) as nat),
            values[(end - 1) as int],
            slice,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn state_satisfied(state: QualificationState) -> bool {
    state.ready_count > 0
        && state.stale_count == 0
        && state.mismatched_count == 0
        && state.unreviewed_count == 0
        && state.not_ready_count == 0
        && !state.conflicting
}

pub open spec fn corresponds(
    state: QualificationState,
    ready_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    unreviewed_count: u16,
    not_ready_count: u16,
    conflicting: bool,
    first_report: Option<Sha256Digest>,
    first_verdict: Option<QualificationVerdict>,
    aggregate_digest: Seq<u8>,
) -> bool {
    state.ready_count == ready_count
        && state.stale_count == stale_count
        && state.mismatched_count == mismatched_count
        && state.unreviewed_count == unreviewed_count
        && state.not_ready_count == not_ready_count
        && state.conflicting == conflicting
        && state.first_report == first_report
        && state.first_verdict == first_verdict
        && state.aggregate_digest == aggregate_digest
}

pub open spec fn qualification_satisfied(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    state_satisfied(through(values, slice, candidate, evaluated_at, values.len() as nat))
}

} // verus!
