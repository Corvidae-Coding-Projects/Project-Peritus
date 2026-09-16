//! Independent terminal-priority matrices through reduction, replay, and state decoding.

mod support;

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_scheduler::{
    DispatchId, FailureDisposition, RecoveryPolicy, SchedulerCommandKind, SchedulerErrorKind,
    SchedulerEvent, SchedulerState, SchedulerStateFrame, SchedulerTerminalKind, WorkId, decide,
    pending_directives, replay,
};

use support::{Fixture, bytes, digest};

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Succeeded,
    Cancelled,
    Failed,
    Abandoned,
    DependencyFailed,
    Exhausted,
    Ambiguous,
}

struct Scenario {
    fixture: Fixture,
    state: SchedulerState,
    events: Vec<SchedulerEvent>,
    next_command: u8,
    expected_non_success: Vec<WorkId>,
}

impl Scenario {
    fn new() -> Self {
        let fixture = Fixture::new();
        let (state, events) = fixture.started();
        let mut scenario =
            Self { fixture, state, events, next_command: 3, expected_non_success: Vec::new() };
        scenario.apply(SchedulerCommandKind::RegisterWorker {
            descriptor: scenario.fixture.worker(30, 1),
        });
        scenario
    }

    fn apply(&mut self, kind: SchedulerCommandKind) {
        Fixture::apply(&mut self.state, &mut self.events, self.next_command, kind);
        self.next_command += 1;
    }

    fn add_outcome(&mut self, id: u8, outcome: Outcome) {
        let work_id = WorkId::new(bytes(id)).expect("valid fixed work identity");
        self.apply(SchedulerCommandKind::AdmitWork {
            spec: self.fixture.work(id, 1, Vec::new(), None, 2, RecoveryPolicy::Fail),
        });
        if !matches!(outcome, Outcome::Succeeded) {
            self.expected_non_success.push(work_id);
        }
        match outcome {
            Outcome::Cancelled => self.apply(SchedulerCommandKind::CancelWork { work_id }),
            Outcome::Exhausted => {
                self.apply(SchedulerCommandKind::ExhaustWork { work_id, cause_digest: digest(id) });
            }
            Outcome::DependencyFailed => {
                let dependent = WorkId::new(bytes(id + 1)).expect("valid fixed dependent identity");
                self.apply(SchedulerCommandKind::AdmitWork {
                    spec: self.fixture.work(
                        id + 1,
                        1,
                        vec![work_id],
                        None,
                        2,
                        RecoveryPolicy::Fail,
                    ),
                });
                self.expected_non_success.push(dependent);
                self.apply(SchedulerCommandKind::CancelWork { work_id });
            }
            Outcome::Succeeded | Outcome::Failed | Outcome::Abandoned | Outcome::Ambiguous => {
                let dispatch_id =
                    DispatchId::new(bytes(id + 100)).expect("valid fixed dispatch identity");
                self.apply(SchedulerCommandKind::DispatchNext {
                    dispatch_id,
                    dispatch_token: digest(id),
                });
                self.apply(SchedulerCommandKind::AcknowledgeStart { dispatch_id });
                let command = match outcome {
                    Outcome::Succeeded => SchedulerCommandKind::CompleteWork {
                        dispatch_id,
                        result_digest: digest(id),
                    },
                    Outcome::Abandoned => SchedulerCommandKind::AbandonDispatch {
                        dispatch_id,
                        cause_digest: digest(id),
                    },
                    Outcome::Ambiguous => SchedulerCommandKind::FailWork {
                        dispatch_id,
                        failure_digest: digest(id),
                        disposition: FailureDisposition::Ambiguous,
                    },
                    Outcome::Failed => SchedulerCommandKind::FailWork {
                        dispatch_id,
                        failure_digest: digest(id),
                        disposition: FailureDisposition::Failed,
                    },
                    Outcome::Cancelled | Outcome::DependencyFailed | Outcome::Exhausted => {
                        panic!("inactive outcomes are handled before dispatch")
                    }
                };
                self.apply(command);
            }
        }
    }

    fn finalize_and_check(&mut self, expected: SchedulerTerminalKind) {
        self.apply(SchedulerCommandKind::FinalizeScheduler);
        self.expected_non_success.sort();
        let terminal = self.state.terminal().expect("finalization retains a terminal summary");
        assert_eq!(terminal.kind(), expected);
        assert_eq!(terminal.non_successful_work(), self.expected_non_success);
        assert!(self.state.reservations().is_empty());
        assert!(pending_directives(&self.state).is_empty());
        assert_eq!(replay(&self.events).expect("accepted history replays"), self.state);

        let encoded =
            encode_message(&SchedulerStateFrame::from_state(&self.state), CodecLimits::PRODUCTION)
                .expect("bounded final state encodes");
        let decoded = decode_message::<SchedulerStateFrame>(&encoded, CodecLimits::PRODUCTION)
            .expect("decoded state validates the same terminal and digest");
        assert!(decoded.matches_state(&self.state));
    }
}

#[test]
fn every_terminal_pair_uses_declared_priority_and_exact_non_success_ids() {
    // The expected ordering is the public outcome policy, independent of evaluator helpers.
    let cases = [
        (Outcome::Succeeded, SchedulerTerminalKind::Completed),
        (Outcome::Cancelled, SchedulerTerminalKind::Cancelled),
        (Outcome::Failed, SchedulerTerminalKind::Failed),
        (Outcome::Abandoned, SchedulerTerminalKind::Failed),
        (Outcome::DependencyFailed, SchedulerTerminalKind::DependencyFailed),
        (Outcome::Exhausted, SchedulerTerminalKind::Exhausted),
        (Outcome::Ambiguous, SchedulerTerminalKind::Ambiguous),
    ];
    for (left_index, &(left, left_kind)) in cases.iter().enumerate() {
        for (right_index, &(right, right_kind)) in cases.iter().enumerate() {
            let mut scenario = Scenario::new();
            // Reverse identity order makes admission order differ from canonical output order.
            scenario.add_outcome(80, left);
            scenario.add_outcome(40, right);
            let expected = if left_index >= right_index { left_kind } else { right_kind };
            scenario.finalize_and_check(expected);
        }
    }
}

#[test]
fn empty_scheduler_completes_with_an_empty_non_success_list() {
    let mut scenario = Scenario::new();
    scenario.finalize_and_check(SchedulerTerminalKind::Completed);
    // SHA-256 of the fixed v1 domain, Completed tag 1, and an eight-byte zero count.
    assert_eq!(
        scenario.state.terminal().expect("finalized summary").digest().as_bytes(),
        &[
            0xef, 0x4d, 0xd6, 0xb7, 0x3b, 0x8d, 0xbe, 0xe5, 0x24, 0xe5, 0xd4, 0x3b, 0xec, 0xa3,
            0x9d, 0xa1, 0xf8, 0xdf, 0x56, 0xf8, 0x09, 0x3a, 0x57, 0x45, 0x56, 0x60, 0x50, 0x31,
            0xad, 0xfb, 0x59, 0xcf,
        ]
    );
}

#[test]
fn mixed_terminal_preserves_the_v1_digest_of_canonical_work_ids() {
    let mut scenario = Scenario::new();
    scenario.add_outcome(80, Outcome::Cancelled);
    scenario.add_outcome(40, Outcome::Failed);
    scenario.finalize_and_check(SchedulerTerminalKind::Failed);
    // Fixed v1 domain, Failed tag 2, u64be count 2, then [40; 16] and [80; 16].
    assert_eq!(
        scenario.state.terminal().expect("finalized summary").digest().as_bytes(),
        &[
            0xb2, 0xa9, 0xeb, 0x27, 0xa1, 0xa0, 0x90, 0x96, 0x7f, 0x35, 0xe6, 0x52, 0x3d, 0xff,
            0x85, 0x3d, 0x18, 0x59, 0x2b, 0x50, 0x13, 0x4c, 0x61, 0x8b, 0x1d, 0xd1, 0x06, 0xc0,
            0xc4, 0xc2, 0x52, 0x4d,
        ]
    );
}

#[test]
fn missing_terminal_evidence_cannot_finalize_or_change_input() {
    let mut scenario = Scenario::new();
    scenario.apply(SchedulerCommandKind::AdmitWork {
        spec: scenario.fixture.work(40, 1, Vec::new(), None, 2, RecoveryPolicy::Fail),
    });
    let before = scenario.state.clone();
    let command = Fixture::command(
        &scenario.state,
        scenario.next_command,
        SchedulerCommandKind::FinalizeScheduler,
    );
    let error = decide(&scenario.state, &command).expect_err("queued work has no success evidence");
    assert_eq!(error.kind(), SchedulerErrorKind::IllegalTransition);
    assert_eq!(scenario.state, before);
    assert!(scenario.state.terminal().is_none());
}
