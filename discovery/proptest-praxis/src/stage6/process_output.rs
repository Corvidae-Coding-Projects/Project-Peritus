//! Generated output-policy and lifecycle histories with an independent byte oracle.

use crate::Metrics;
use peritus_process::{
    CancellationReason, LifecyclePhase, LifecycleState, OutputOverflowAction, OutputPolicy,
    OutputStream,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub bytes: Vec<u8>,
    pub stderr: bool,
    pub chunk: u8,
    pub window: u8,
    pub spool: u8,
    pub stream_ceiling: u8,
    pub events: u8,
    pub terminate: bool,
    pub lifecycle: Vec<LifecycleStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LifecycleStep {
    Advance(u8),
    Stop { sequence: u8, reason: u8 },
    SpawnFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedOutput {
    pub stream: OutputStream,
    pub observed: u64,
    pub spooled: Vec<u8>,
    pub tail: Vec<u8>,
    pub dropped: u64,
}

fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let phase = prop_oneof![
        7 => (0_u8..=6).prop_map(LifecycleStep::Advance),
        3 => (0_u8..=20, 0_u8..=4)
            .prop_map(|(sequence, reason)| LifecycleStep::Stop { sequence, reason }),
        1 => Just(LifecycleStep::SpawnFailed),
    ];
    let broad = (
        prop::collection::vec(any::<u8>(), 0..=128),
        any::<bool>(),
        0_u8..=130,
        0_u8..=130,
        0_u8..=130,
        0_u8..=130,
        0_u8..=16,
        any::<bool>(),
        prop::collection::vec(phase, 0..=max_steps.min(40)),
    )
        .prop_map(
            |(
                bytes,
                stderr,
                chunk,
                window,
                spool,
                stream_ceiling,
                events,
                terminate,
                lifecycle,
            )| {
                Case {
                    bytes,
                    stderr,
                    chunk,
                    window,
                    spool,
                    stream_ceiling,
                    events,
                    terminate,
                    lifecycle,
                }
            },
        )
        .boxed();
    let window_boundary = broad.clone().prop_map(|mut case| {
        case.window = case.window.max(1);
        case.chunk = case.window.saturating_add(1);
        case.spool = case.chunk;
        case.events = case.events.max(1);
        case
    });
    let live_lifecycle = (broad.clone(), 0_u8..=4, 0_u8..=4, any::<bool>()).prop_map(
        |(mut case, first_reason, later_reason, spawn_failed)| {
            // Reach live states before injecting competing stops. Independent
            // random phases spent most cases rejecting operations at Authorized.
            case.chunk = case.chunk.max(1);
            case.window = case.window.max(case.chunk);
            case.spool = case.spool.max(case.chunk);
            case.events = case.events.max(1);
            case.lifecycle = if spawn_failed {
                vec![
                    LifecycleStep::Advance(1),
                    LifecycleStep::SpawnFailed,
                    LifecycleStep::Advance(6),
                    LifecycleStep::Advance(6),
                ]
            } else {
                vec![
                    LifecycleStep::Advance(1),
                    LifecycleStep::Advance(2),
                    LifecycleStep::Stop { sequence: 10, reason: first_reason },
                    LifecycleStep::Stop { sequence: 1, reason: later_reason },
                    LifecycleStep::Advance(4),
                    LifecycleStep::Advance(5),
                    LifecycleStep::Advance(6),
                    LifecycleStep::Advance(6),
                ]
            };
            case
        },
    );
    prop_oneof![5 => broad, 2 => window_boundary, 3 => live_lifecycle].boxed()
}

pub fn expected(case: &Case) -> ExpectedOutput {
    let accepted =
        case.bytes.len().min(usize::from(case.spool)).min(usize::from(case.stream_ceiling));
    let tail_start = case.bytes.len().saturating_sub(usize::from(case.window));
    ExpectedOutput {
        stream: if case.stderr { OutputStream::Stderr } else { OutputStream::Stdout },
        observed: case.bytes.len() as u64,
        spooled: case.bytes[..accepted].to_vec(),
        tail: case.bytes[tail_start..].to_vec(),
        dropped: (case.bytes.len() - accepted) as u64,
    }
}

fn phase(value: u8) -> LifecyclePhase {
    match value {
        0 => LifecyclePhase::Authorized,
        1 => LifecyclePhase::Starting,
        2 => LifecyclePhase::Running,
        3 => LifecyclePhase::Stopping,
        4 => LifecyclePhase::Exited,
        5 => LifecyclePhase::Closed,
        _ => LifecyclePhase::Terminal,
    }
}

fn reason(value: u8) -> CancellationReason {
    match value {
        0 => CancellationReason::User,
        1 => CancellationReason::Deadline,
        2 => CancellationReason::OutputLimit,
        3 => CancellationReason::ResourceLimit,
        _ => CancellationReason::BackendFailure,
    }
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.bytes.len() > 128 || case.lifecycle.len() > 40 {
        return Err("process-output fixture exceeds bound".into());
    }
    let mut metrics = Metrics::new();
    let valid = case.chunk > 0
        && case.window > 0
        && case.spool > 0
        && case.events > 0
        && case.chunk <= case.window
        && case.chunk <= case.spool;
    let action = if case.terminate {
        OutputOverflowAction::Terminate
    } else {
        OutputOverflowAction::ContinueIncomplete
    };
    let policy = OutputPolicy::new(
        u64::from(case.chunk),
        u64::from(case.window),
        u64::from(case.spool),
        u64::from(case.events),
        u64::from(case.stream_ceiling),
        u64::from(case.stream_ceiling),
        u64::from(case.stream_ceiling),
        action,
    );
    if policy.is_ok() != valid {
        return Err(format!("output policy validity mismatch: expected {valid}, got {policy:?}"));
    }
    if let Ok(policy) = policy {
        if policy.chunk_bytes() != u64::from(case.chunk)
            || policy.retained_window_bytes() != u64::from(case.window)
            || policy.spool_bytes() != Some(u64::from(case.spool))
            || policy.event_count() != u64::from(case.events)
            || policy.stdout_bytes() != Some(u64::from(case.stream_ceiling))
            || policy.stderr_bytes() != Some(u64::from(case.stream_ceiling))
            || policy.terminal_bytes() != Some(u64::from(case.stream_ceiling))
            || policy.overflow_action() != action
        {
            return Err("output policy did not preserve checked bounds".into());
        }
        let expected = expected(case);
        if expected.dropped > 0 {
            bump(&mut metrics, "expected_truncated_outputs");
        } else {
            bump(&mut metrics, "expected_complete_outputs");
        }
        bump(&mut metrics, if case.stderr { "stderr_cases" } else { "stdout_cases" });
        bump(&mut metrics, "accepted_policies");
    } else {
        bump(&mut metrics, "rejected_policies");
    }

    let mut actual = LifecycleState::authorized();
    let mut state = LifecyclePhase::Authorized;
    let mut trigger: Option<(u64, CancellationReason)> = None;
    let mut terminal = false;
    for (index, step) in case.lifecycle.iter().enumerate() {
        match step {
            LifecycleStep::Advance(value) => {
                let next = phase(*value);
                let legal = matches!(
                    (state, next),
                    (LifecyclePhase::Authorized, LifecyclePhase::Starting)
                        | (LifecyclePhase::Starting, LifecyclePhase::Running)
                        | (
                            LifecyclePhase::Running | LifecyclePhase::Stopping,
                            LifecyclePhase::Exited
                        )
                        | (LifecyclePhase::Exited, LifecyclePhase::Closed)
                        | (LifecyclePhase::Closed, LifecyclePhase::Terminal)
                ) && !(next == LifecyclePhase::Terminal && terminal);
                let result = actual.advance(next);
                if result.is_ok() != legal {
                    return Err(format!("lifecycle step {index}: advance validity mismatch"));
                }
                if legal {
                    state = next;
                    terminal |= next == LifecyclePhase::Terminal;
                }
            }
            LifecycleStep::Stop { sequence, reason: selected } => {
                let accepted = trigger.is_none()
                    && matches!(state, LifecyclePhase::Starting | LifecyclePhase::Running);
                let selected = reason(*selected);
                let observed = actual.request_stop(u64::from(*sequence), selected);
                if observed != accepted {
                    return Err(format!("lifecycle step {index}: stop precedence mismatch"));
                }
                if accepted {
                    trigger = Some((u64::from(*sequence), selected));
                    state = LifecyclePhase::Stopping;
                    bump(&mut metrics, "accepted_stops");
                }
            }
            LifecycleStep::SpawnFailed => {
                let legal = state == LifecyclePhase::Starting;
                if actual.spawn_failed().is_ok() != legal {
                    return Err(format!("lifecycle step {index}: spawn failure validity mismatch"));
                }
                if legal {
                    state = LifecyclePhase::Closed;
                }
            }
        }
        if actual.phase() != state || actual.terminal_accepted() != terminal {
            return Err(format!("lifecycle step {index}: state drift"));
        }
        let observed = actual.first_trigger().map(|record| (record.sequence(), record.reason()));
        if observed != trigger {
            return Err(format!("lifecycle step {index}: first trigger drift"));
        }
        bump(&mut metrics, "lifecycle_steps");
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    vec![
        Case {
            bytes: b"abcdefghij".to_vec(),
            stderr: false,
            chunk: 4,
            window: 6,
            spool: 8,
            stream_ceiling: 7,
            events: 4,
            terminate: false,
            lifecycle: vec![
                LifecycleStep::Advance(1),
                LifecycleStep::Advance(2),
                LifecycleStep::Stop { sequence: 1, reason: 2 },
                LifecycleStep::Stop { sequence: 2, reason: 0 },
                LifecycleStep::Advance(4),
                LifecycleStep::Advance(5),
                LifecycleStep::Advance(6),
            ],
        },
        Case {
            bytes: b"stderr-tail".to_vec(),
            stderr: true,
            chunk: 1,
            window: 4,
            spool: 16,
            stream_ceiling: 16,
            events: 3,
            terminate: false,
            lifecycle: vec![LifecycleStep::Advance(1), LifecycleStep::SpawnFailed],
        },
        Case {
            bytes: Vec::new(),
            stderr: false,
            chunk: 5,
            window: 4,
            spool: 8,
            stream_ceiling: 8,
            events: 1,
            terminate: true,
            lifecycle: vec![],
        },
    ]
}

pub fn native_cases() -> Vec<Case> {
    let case = |bytes: &[u8], stderr, chunk, window, spool, stream_ceiling, terminate| Case {
        bytes: bytes.to_vec(),
        stderr,
        chunk,
        window,
        spool,
        stream_ceiling,
        events: 16,
        terminate,
        lifecycle: Vec::new(),
    };
    vec![
        case(b"", false, 2, 4, 8, 8, false),
        case(b"abc", false, 2, 4, 8, 8, false),
        case(b"abcdefghij", false, 4, 5, 16, 16, false),
        case(b"ABCDEFGHIJ", true, 4, 6, 8, 16, false),
        case(b"0123456789", true, 4, 6, 16, 5, false),
        case(b"abcdefghijklmnop", false, 16, 16, 16, 6, true),
    ]
}

pub fn calibration() -> Case {
    Case {
        bytes: b"calibration".to_vec(),
        stderr: false,
        chunk: 6,
        window: 5,
        spool: 8,
        stream_ceiling: 8,
        events: 1,
        terminate: false,
        lifecycle: vec![],
    }
}
