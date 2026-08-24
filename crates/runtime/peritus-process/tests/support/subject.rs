use peritus_conformance::{
    ProcessConformanceError, ProcessConformanceFixture, ProcessConformanceObservation,
    ProcessConformanceSubject, ProcessDisposition, ProcessEffectObservation,
    ProcessInvocationObservation, ProcessIoMode, ProcessOutputObservation, ProcessOutputStream,
    ProcessOwnershipObservation, ProcessScenario, ProcessStreamOffsetObservation, ProcessTrigger,
};
use peritus_process::{
    CancellationReason, ExecutionAuthorizationRequest, ExecutionGateway, GracefulAction, IoMode,
    OutputStream, OwnedProcess, ProcessCursor, ProcessEvent, ProcessEventKind,
    ProcessResourceDimension, ProcessStore, StdinPolicy, StopTrigger, TerminalDisposition,
    TerminalSize,
};

use super::{Ids, PlanOptions, TestRoot, commit_authority, intent, open_journal, plan};

pub struct ProductionProcessSubject {
    sequence: u8,
}

impl ProductionProcessSubject {
    pub const fn new() -> Self {
        Self { sequence: 1 }
    }

    fn next_ids(&mut self) -> Ids {
        let seed = self.sequence;
        self.sequence = self.sequence.wrapping_add(23).max(1);
        Ids::new(seed)
    }
}

impl ProcessConformanceSubject for ProductionProcessSubject {
    fn exercise(
        &mut self,
        fixture: &ProcessConformanceFixture,
    ) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
        let root = TestRoot::new();
        let ids = self.next_ids();
        match fixture.scenario() {
            ProcessScenario::Authorization(drift) => {
                super::subject_authorization::exercise(&root, &ids, drift)
            }
            ProcessScenario::Restart(probe) => {
                super::subject_recovery::exercise(&root, &ids, probe)
            }
            _ => execute(&root, &ids, fixture),
        }
    }
}

fn execute(
    root: &TestRoot,
    ids: &Ids,
    fixture: &ProcessConformanceFixture,
) -> Result<ProcessConformanceObservation, ProcessConformanceError> {
    let scenario = fixture.scenario();
    let execution = execution_plan(root, ids, fixture)?;
    let owned = launch(root, ids, execution)?;
    let control = owned.control();
    if scenario == ProcessScenario::PtyStreaming {
        std::thread::sleep(std::time::Duration::from_millis(20));
        control
            .resize(TerminalSize::new(30, 100, 0, 0).map_err(|_| infrastructure())?)
            .map_err(|_| infrastructure())?;
    }
    if !fixture.stdin().is_empty() {
        control.write_stdin(fixture.stdin().to_vec()).map_err(|_| infrastructure())?;
        control.close_stdin().map_err(|_| infrastructure())?;
    }
    if matches!(scenario, ProcessScenario::Cancellation | ProcessScenario::TerminalUniqueness) {
        control.cancel(CancellationReason::User).map_err(|_| infrastructure())?;
    }
    let terminal = owned.wait().map_err(|_| infrastructure())?;
    let events = control.read_events(ProcessCursor::after(0), 512);
    let invocation = invocation(root, fixture, &events)?;
    let output = output_observation(&terminal, &events);
    let descendants_observed = terminal
        .resources()
        .iter()
        .find(|value| value.dimension() == ProcessResourceDimension::ProcessCount)
        .map_or(0, |value| value.value().saturating_sub(1));
    let ownership = ProcessOwnershipObservation::new(
        descendants_observed,
        terminal.tree_cleanup_complete(),
        terminal.support_tasks_joined(),
        u64::from(control.terminal_result().is_some()),
        terminal.escalation().graceful_attempted(),
        terminal.escalation().forced(),
    );
    Ok(ProcessConformanceObservation::new(
        disposition(terminal.disposition()),
        trigger(terminal.first_trigger()),
        invocation,
        output,
        ownership,
        ProcessEffectObservation::new(1, 1, 1, true),
        None,
        false,
        false,
    ))
}

fn execution_plan(
    root: &TestRoot,
    ids: &Ids,
    fixture: &ProcessConformanceFixture,
) -> Result<peritus_process::ExecutionPlan, ProcessConformanceError> {
    let scenario = fixture.scenario();
    let (arguments, timeout, graceful, grace, processes, descendants) = match scenario {
        ProcessScenario::LiteralInvocation => (
            std::iter::once("literal".to_owned())
                .chain(fixture.arguments().iter().map(|value| (*value).to_owned()))
                .collect(),
            None,
            GracefulAction::Terminate,
            100,
            1,
            0,
        ),
        ProcessScenario::TreeCleanup => (
            vec!["tree".to_owned(), fixture.descendant_depth().to_string()],
            None,
            GracefulAction::Terminate,
            100,
            fixture.descendant_depth().saturating_add(1),
            u32::try_from(fixture.descendant_depth()).map_err(|_| infrastructure())?,
        ),
        ProcessScenario::Deadline => {
            (vec!["control".to_owned()], Some(50), GracefulAction::CloseInput, 20, 1, 0)
        }
        _ => (
            fixture.arguments().iter().map(|value| (*value).to_owned()).collect(),
            None,
            GracefulAction::Terminate,
            100,
            1,
            0,
        ),
    };
    let io = match fixture.io_mode() {
        ProcessIoMode::Pipes => IoMode::Pipes,
        ProcessIoMode::Pty => {
            IoMode::Pty(TerminalSize::new(24, 80, 0, 0).map_err(|_| infrastructure())?)
        }
    };
    let stdin = if fixture.stdin().is_empty() {
        StdinPolicy::Closed
    } else {
        let size = u64::try_from(fixture.stdin().len()).map_err(|_| infrastructure())?;
        StdinPolicy::bounded(size, size).map_err(|_| infrastructure())?
    };
    let output_limit =
        if scenario == ProcessScenario::LiteralInvocation { 4_096 } else { fixture.output_limit() };
    plan(
        root,
        ids,
        PlanOptions {
            arguments,
            environment: fixture
                .environment()
                .iter()
                .map(|item| (item.name(), item.value()))
                .collect(),
            io,
            stdin,
            output_limit,
            wall_timeout: timeout,
            graceful,
            grace_millis: grace,
            process_count: processes,
            descendants,
            workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
            resize_allowed: true,
            environment_authority: None,
            resource_fidelity: peritus_sandbox::ResourceFidelity::Reference,
        },
    )
    .map_err(|_| infrastructure())
}

pub fn launch(
    root: &TestRoot,
    ids: &Ids,
    execution: peritus_process::ExecutionPlan,
) -> Result<OwnedProcess, ProcessConformanceError> {
    let action = intent(ids, &execution);
    let mut journal = open_journal(root);
    let receipts =
        commit_authority(&mut journal, ids, &action, execution.resource_policy().wall_millis());
    let gateway = ExecutionGateway::new(
        ProcessStore::open(root.registry(), root.workspace()).map_err(|_| infrastructure())?,
    );
    let request = ExecutionAuthorizationRequest::new(
        &action,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    );
    gateway.launch(&request, execution).map_err(|_| infrastructure())
}

fn invocation(
    root: &TestRoot,
    fixture: &ProcessConformanceFixture,
    events: &[ProcessEvent],
) -> Result<ProcessInvocationObservation, ProcessConformanceError> {
    let command = std::iter::once(fixture.executable().to_owned())
        .chain(fixture.arguments().iter().map(|value| (*value).to_owned()))
        .collect();
    let environment = fixture
        .environment()
        .iter()
        .map(|value| (value.name().to_owned(), value.value().to_owned()))
        .collect();
    if fixture.scenario() == ProcessScenario::LiteralInvocation {
        let workspace = std::fs::canonicalize(root.workspace()).map_err(|_| infrastructure())?;
        let fields: Vec<_> = stream(events, OutputStream::Stdout)
            .split(|byte| *byte == 0)
            .filter(|field| !field.is_empty())
            .map(|field| String::from_utf8(field.to_vec()))
            .collect::<Result<_, _>>()
            .map_err(|_| infrastructure())?;
        let expected_count = fixture.arguments().len() + 3;
        if fields.len() != expected_count
            || fields[0] != workspace.to_string_lossy()
            || fields[1] != fixture.environment()[0].value()
            || fields[2] != fixture.environment()[1].value()
            || !fields[3..].iter().map(String::as_str).eq(fixture.arguments().iter().copied())
        {
            return Err(infrastructure());
        }
    }
    Ok(ProcessInvocationObservation::new(
        command,
        fixture.working_directory().to_owned(),
        environment,
        false,
    ))
}

fn output_observation(
    terminal: &peritus_process::TerminalResult,
    events: &[ProcessEvent],
) -> ProcessOutputObservation {
    let offsets = events
        .iter()
        .filter_map(|event| {
            let offset = event.stream_offset()?;
            let ProcessEventKind::Output(stream) = event.kind() else { return None };
            let stream = match stream {
                OutputStream::Stdout => ProcessOutputStream::Stdout,
                OutputStream::Stderr => ProcessOutputStream::Stderr,
                OutputStream::Terminal => ProcessOutputStream::Terminal,
            };
            Some(ProcessStreamOffsetObservation::new(stream, offset))
        })
        .collect();
    let observed = terminal.output().streams().iter().map(|value| value.observed()).sum();
    let retained = terminal.output().streams().iter().map(|value| value.retained()).sum();
    let dropped = terminal.output().streams().iter().map(|value| value.dropped()).sum();
    ProcessOutputObservation::new(
        stream(events, OutputStream::Stdout),
        stream(events, OutputStream::Stderr),
        stream(events, OutputStream::Terminal),
        events.iter().map(ProcessEvent::sequence).collect(),
        offsets,
        observed,
        retained,
        dropped,
        terminal.output().is_complete(),
        events.iter().any(|event| matches!(event.kind(), ProcessEventKind::StdinClosed)),
        events.iter().any(|event| matches!(event.kind(), ProcessEventKind::Resized(_))),
    )
}

fn stream(events: &[ProcessEvent], expected: OutputStream) -> Vec<u8> {
    events
        .iter()
        .filter_map(|event| {
            matches!(event.kind(), ProcessEventKind::Output(stream) if *stream == expected)
                .then_some(event.data())
        })
        .flatten()
        .copied()
        .collect()
}

const fn disposition(value: TerminalDisposition) -> ProcessDisposition {
    match value {
        TerminalDisposition::Cancelled => ProcessDisposition::Cancelled,
        TerminalDisposition::TimedOut => ProcessDisposition::TimedOut,
        TerminalDisposition::OutputLimit => ProcessDisposition::OutputLimit,
        _ => ProcessDisposition::Exited,
    }
}

fn trigger(value: Option<StopTrigger>) -> Option<ProcessTrigger> {
    match value.map(StopTrigger::reason) {
        Some(CancellationReason::User) => Some(ProcessTrigger::User),
        Some(CancellationReason::Deadline) => Some(ProcessTrigger::Deadline),
        Some(CancellationReason::OutputLimit) => Some(ProcessTrigger::OutputLimit),
        _ => None,
    }
}

const fn infrastructure() -> ProcessConformanceError {
    ProcessConformanceError::Infrastructure
}
