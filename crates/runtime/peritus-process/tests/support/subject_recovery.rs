use std::time::{Duration, Instant};

use peritus_conformance::{
    ProcessConformanceError, ProcessConformanceObservation, ProcessDisposition,
    ProcessEffectObservation, ProcessInvocationObservation, ProcessOutputObservation,
    ProcessOwnershipObservation, ProcessRecoveryDisposition, ProcessRecoveryProbe,
};
use peritus_process::{
    CancellationReason, GracefulAction, IoMode, ProbeObservation, ProcessCursor, ProcessEventKind,
    ProcessProbe, ProcessStore, ProcessTreeIdentity, RecoveryDisposition, StdinPolicy,
};

use super::{Ids, PlanOptions, TestRoot, plan, subject};

pub fn exercise(
    root: &TestRoot,
    ids: &Ids,
    requested: ProcessRecoveryProbe,
) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
    let execution = plan(
        root,
        ids,
        PlanOptions {
            arguments: if requested == ProcessRecoveryProbe::Terminal {
                vec!["tree".to_owned(), "0".to_owned()]
            } else {
                vec!["control".to_owned()]
            },
            environment: Vec::new(),
            io: IoMode::Pipes,
            stdin: StdinPolicy::Closed,
            output_limit: 64,
            wall_timeout: None,
            graceful: GracefulAction::Terminate,
            grace_millis: 50,
            process_count: 1,
            descendants: 0,
            workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .map_err(|error| failed(requested, "create plan", error))?;
    let mut owned = Some(
        subject::launch(root, ids, execution)
            .map_err(|error| failed(requested, "launch owner", error))?,
    );
    let process = owned.as_ref().ok_or_else(infrastructure)?;
    let control = process.control();
    if requested == ProcessRecoveryProbe::Terminal {
        owned
            .take()
            .ok_or_else(infrastructure)?
            .wait()
            .map_err(|error| failed(requested, "wait for terminal", error))?;
    } else {
        wait_for_start(&control).map_err(|error| failed(requested, "wait for start", error))?;
    }
    let store = ProcessStore::open(root.registry(), root.workspace())
        .map_err(|error| failed(requested, "reopen store", error))?;
    let mut probe = FixedProbe::new(requested);
    let report =
        store.reconcile(&mut probe).map_err(|error| failed(requested, "reconcile", error))?;
    let entry = report.entries().first().copied().ok_or_else(infrastructure)?;
    // The catalog reports one failed contract. Keep the exact subprobe evidence in the
    // test's captured output so a native CI failure identifies the actual classification.
    eprintln!(
        "recovery fixture {requested:?}: {entry:?}; terminal={:?}",
        control.terminal_result()
    );
    if requested != ProcessRecoveryProbe::Terminal {
        let _ = control.cancel(CancellationReason::SupervisorShutdown);
        let _ = owned.take().ok_or_else(infrastructure)?.wait();
    }
    Ok(ProcessConformanceObservation::new(
        ProcessDisposition::Recovered,
        None,
        ProcessInvocationObservation::new(Vec::new(), String::new(), Vec::new(), false),
        ProcessOutputObservation::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            0,
            0,
            0,
            true,
            false,
            false,
        ),
        ProcessOwnershipObservation::new(0, true, true, 0, false, false),
        ProcessEffectObservation::new(1, 1, 1, true),
        Some(recovery_disposition(entry.disposition())),
        false,
        entry.signal_sent(),
    ))
}

fn wait_for_start(
    control: &peritus_process::ProcessControl,
) -> Result<(), ProcessConformanceError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut cursor = ProcessCursor::after(0);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        for event in control.wait_events(cursor, 32, remaining) {
            cursor = ProcessCursor::after(event.sequence());
            if matches!(event.kind(), ProcessEventKind::Started { .. }) {
                return Ok(());
            }
        }
        if control.terminal_result().is_some() {
            break;
        }
    }
    eprintln!("recovery start was not observed: terminal={:?}", control.terminal_result());
    Err(infrastructure())
}

fn failed(
    probe: ProcessRecoveryProbe,
    stage: &str,
    error: impl std::fmt::Debug,
) -> ProcessConformanceError {
    eprintln!("recovery fixture {probe:?} failed to {stage}: {error:?}");
    infrastructure()
}

struct FixedProbe {
    observation: ProbeObservation,
}

impl FixedProbe {
    const fn new(probe: ProcessRecoveryProbe) -> Self {
        let observation = match probe {
            ProcessRecoveryProbe::ExactLive => ProbeObservation::ExactLive,
            ProcessRecoveryProbe::Absent | ProcessRecoveryProbe::Terminal => {
                ProbeObservation::ExactAbsent
            }
            ProcessRecoveryProbe::Mismatched => ProbeObservation::Mismatched,
            ProcessRecoveryProbe::Unverifiable => ProbeObservation::Unverifiable,
        };
        Self { observation }
    }
}

impl ProcessProbe for FixedProbe {
    fn observe(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<ProbeObservation, peritus_process::ProcessError> {
        Ok(self.observation)
    }

    fn terminate(
        &mut self,
        _identity: ProcessTreeIdentity,
    ) -> Result<(), peritus_process::ProcessError> {
        Ok(())
    }
}

const fn recovery_disposition(value: RecoveryDisposition) -> ProcessRecoveryDisposition {
    match value {
        RecoveryDisposition::AlreadyTerminal => ProcessRecoveryDisposition::Terminal,
        RecoveryDisposition::LiveOwned => ProcessRecoveryDisposition::LiveOwned,
        RecoveryDisposition::AbsentUnobserved => ProcessRecoveryDisposition::AbsentUnobserved,
        RecoveryDisposition::Indeterminate => ProcessRecoveryDisposition::Indeterminate,
    }
}

const fn infrastructure() -> ProcessConformanceError {
    ProcessConformanceError::Infrastructure
}
