use std::{thread, time::Duration};

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
    .map_err(|_| infrastructure())?;
    let mut owned = Some(subject::launch(root, ids, execution)?);
    let process = owned.as_ref().ok_or_else(infrastructure)?;
    let control = process.control();
    if requested == ProcessRecoveryProbe::Terminal {
        owned.take().ok_or_else(infrastructure)?.wait().map_err(|_| infrastructure())?;
    } else {
        wait_for_start(&control)?;
    }
    let store =
        ProcessStore::open(root.registry(), root.workspace()).map_err(|_| infrastructure())?;
    let mut probe = FixedProbe::new(requested);
    let report = store.reconcile(&mut probe).map_err(|_| infrastructure())?;
    let entry = report.entries().first().copied().ok_or_else(infrastructure)?;
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
    for _ in 0..100 {
        if control
            .read_events(ProcessCursor::after(0), 32)
            .iter()
            .any(|event| matches!(event.kind(), ProcessEventKind::Started { .. }))
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(5));
    }
    Err(infrastructure())
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
