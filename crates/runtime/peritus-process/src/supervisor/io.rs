//! Bounded process input, output-reader tasks, and spool accounting.

use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        Arc,
        mpsc::{self, Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    CancellationReason, LifecyclePhase, LifecycleState, OutputOverflowAction,
    OutputPolicy, OutputStream, ProcessError, ProcessEventKind, ProcessStore, StdinPolicy,
    StopStrategy, StreamAccounting,
    control::{ControlCommand, InputCommand, InputLane, InputOwner, SharedObservation},
    output::{OutputAccounting, RetainedWindow, SpoolSet},
    platform::{self, PlatformProcess},
};

use super::{SupervisorPlan, emit, supervisor_error};

const IO_DRAIN_BUDGET: usize = 8;
const INPUT_RESULT_QUEUE: usize = 64;

pub(super) enum ReaderMessage {
    Data(OutputStream, Vec<u8>),
    Eof,
    Failed,
}

pub(super) struct ReaderTasks {
    pub(super) tasks: Vec<JoinHandle<()>>,
    pub(super) startup_failed: bool,
}

pub(super) fn start_readers(
    readers: Vec<platform::OutputReader>,
    policy: OutputPolicy,
    sender: &SyncSender<ReaderMessage>,
) -> ReaderTasks {
    let Ok(chunk) = usize::try_from(policy.chunk_bytes()) else {
        return ReaderTasks { tasks: Vec::new(), startup_failed: true };
    };
    let mut tasks = Vec::with_capacity(readers.len());
    for (index, output) in readers.into_iter().enumerate() {
        let sender = sender.clone();
        let task = thread::Builder::new()
            .name(format!("peritus-output-{index}"))
            .spawn(move || read_stream(output, chunk, sender));
        match task {
            Ok(task) => tasks.push(task),
            Err(_) => return ReaderTasks { tasks, startup_failed: true },
        }
    }
    ReaderTasks { tasks, startup_failed: false }
}

fn read_stream(
    mut output: platform::OutputReader,
    chunk: usize,
    sender: SyncSender<ReaderMessage>,
) {
    let mut buffer = vec![0_u8; chunk];
    loop {
        match output.reader.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(ReaderMessage::Eof);
                break;
            }
            Ok(read) => {
                if sender.send(ReaderMessage::Data(output.stream, buffer[..read].to_vec())).is_err()
                {
                    break;
                }
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(super::POLL_MILLIS));
            }
            Err(_) => {
                let _ = sender.send(ReaderMessage::Failed);
                break;
            }
        }
    }
    drop(sender);
}

pub(super) struct InputTask {
    lane: InputLane,
    results: Receiver<InputResult>,
    join: Option<JoinHandle<()>>,
    joined: bool,
}

enum InputResult {
    Write { written: u64, failure: Option<InputFailure> },
    Closed(Result<CloseResult, ProcessError>),
}

struct CloseResult {
    written: u64,
    expected: u64,
    failure: Option<InputFailure>,
    platform_close: bool,
}

#[derive(Clone, Copy)]
enum InputFailure {
    BrokenPipe,
    Failed,
    Stopped,
}

impl InputTask {
    pub(super) fn start(
        input: Option<platform::ProcessInput>,
        owner: InputOwner,
        lane: InputLane,
        policy: StdinPolicy,
    ) -> (Self, bool) {
        let (sender, results) = mpsc::sync_channel(INPUT_RESULT_QUEUE);
        if matches!(policy, StdinPolicy::Closed) {
            owner.finish();
            return (Self { lane, results, join: None, joined: true }, false);
        }
        let Some(input) = input else {
            owner.finish();
            return (Self { lane, results, join: None, joined: false }, true);
        };
        let task = thread::Builder::new()
            .name("peritus-input".to_owned())
            .spawn(move || write_stream(input, owner, sender));
        match task {
            Ok(join) => (Self { lane, results, join: Some(join), joined: false }, false),
            Err(_) => (Self { lane, results, join: None, joined: false }, true),
        }
    }

    pub(super) fn request_stop(&self) -> Result<(), ProcessError> {
        self.lane.request_stop();
        self.wake_blocked_write()
    }

    pub(super) fn activate(&self) {
        self.lane.activate();
    }

    pub(super) fn is_finished(&self) -> bool {
        self.joined || self.join.as_ref().is_some_and(JoinHandle::is_finished)
    }

    pub(super) const fn has_task(&self) -> bool {
        self.join.is_some()
    }

    pub(super) fn join(&mut self, failed: &mut bool) -> bool {
        if self.joined {
            return true;
        }
        let Some(join) = self.join.take() else {
            *failed = true;
            return false;
        };
        self.joined = join.join().is_ok();
        *failed |= !self.joined;
        self.joined
    }

    pub(super) fn wake_blocked_write(&self) -> Result<(), ProcessError> {
        let Some(join) = self.join.as_ref() else { return Ok(()) };
        platform::cancel_input_write(join)
    }

    pub(super) fn discard_results(&self) {
        while self.results.try_recv().is_ok() {}
    }
}

impl Drop for InputTask {
    fn drop(&mut self) {
        if self.joined || self.join.is_none() {
            return;
        }
        self.lane.request_stop();
        while !self.is_finished() {
            let _ = self.wake_blocked_write();
            self.discard_results();
            thread::sleep(Duration::from_millis(super::POLL_MILLIS));
        }
        self.discard_results();
        let mut failed = false;
        let _ = self.join(&mut failed);
    }
}

fn write_stream(
    mut input: platform::ProcessInput,
    owner: InputOwner,
    sender: SyncSender<InputResult>,
) {
    while let Some(command) = owner.next() {
        match command {
            InputCommand::Write(bytes) => {
                let requested = u64::try_from(bytes.len())
                    .expect("accepted stdin length remains representable");
                let (written, failure) = write_one(&mut input, &bytes, &owner);
                owner.finish_write(requested, written, failure.is_some());
                if sender.send(InputResult::Write { written, failure }).is_err()
                    || failure.is_some()
                {
                    break;
                }
            }
            InputCommand::Close => {
                let result = input.close_plan().map(|plan| {
                    let (frame, platform_close) = plan.into_parts();
                    let expected = u64::try_from(frame.len())
                        .expect("the fixed PTY close frame length remains representable");
                    let (written, failure) = if frame.is_empty() {
                        (0, None)
                    } else {
                        write_one(&mut input, &frame, &owner)
                    };
                    CloseResult { written, expected, failure, platform_close }
                });
                drop(input);
                owner.finish();
                let _ = sender.send(InputResult::Closed(result));
                return;
            }
        }
    }
    drop(input);
    owner.finish();
}

fn write_one(
    input: &mut dyn Write,
    bytes: &[u8],
    owner: &InputOwner,
) -> (u64, Option<InputFailure>) {
    let mut written = 0_usize;
    while written < bytes.len() {
        if owner.stop_requested() {
            return (written as u64, Some(InputFailure::Stopped));
        }
        match input.write(&bytes[written..]) {
            Ok(0) => return (written as u64, Some(InputFailure::Failed)),
            Ok(count) => written = written.saturating_add(count),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if !owner.wait_for_retry() {
                    return (written as u64, Some(InputFailure::Stopped));
                }
            }
            Err(error) if error.kind() == ErrorKind::BrokenPipe => {
                return (written as u64, Some(InputFailure::BrokenPipe));
            }
            Err(_) if owner.stop_requested() => {
                return (written as u64, Some(InputFailure::Stopped));
            }
            Err(_) => return (written as u64, Some(InputFailure::Failed)),
        }
    }
    loop {
        if owner.stop_requested() {
            return (written as u64, Some(InputFailure::Stopped));
        }
        match input.flush() {
            Ok(()) => return (written as u64, None),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if !owner.wait_for_retry() {
                    return (written as u64, Some(InputFailure::Stopped));
                }
            }
            Err(error) if error.kind() == ErrorKind::BrokenPipe => {
                return (written as u64, Some(InputFailure::BrokenPipe));
            }
            Err(_) if owner.stop_requested() => {
                return (written as u64, Some(InputFailure::Stopped));
            }
            Err(_) => return (written as u64, Some(InputFailure::Failed)),
        }
    }
}

pub(super) fn drain_input(
    task: &InputTask,
    input_closed: &mut bool,
    process: &mut dyn PlatformProcess,
    plan: &SupervisorPlan,
    shared: &Arc<SharedObservation>,
) -> Result<bool, ProcessError> {
    for _ in 0..IO_DRAIN_BUDGET {
        match task.results.try_recv() {
            Ok(InputResult::Write { written, failure }) => {
                if written > 0 {
                    emit(
                        shared,
                        plan,
                        None,
                        ProcessEventKind::StdinAccepted { bytes: written },
                        Vec::new(),
                    );
                }
                if failure.is_some() && !*input_closed {
                    *input_closed = true;
                    emit(shared, plan, None, ProcessEventKind::StdinClosed, Vec::new());
                }
                match failure {
                    Some(InputFailure::Failed) => {
                        return Err(input_error("stdin write failed"));
                    }
                    Some(InputFailure::BrokenPipe | InputFailure::Stopped) | None => {}
                }
            }
            Ok(InputResult::Closed(result)) => {
                if !*input_closed {
                    *input_closed = true;
                    emit(shared, plan, None, ProcessEventKind::StdinClosed, Vec::new());
                }
                let CloseResult { written, expected, failure, platform_close } = match result {
                    Ok(result) => result,
                    Err(error) => return Err(error),
                };
                match failure {
                    Some(InputFailure::Failed) => {
                        return Err(input_error("stdin close sequence failed"));
                    }
                    None if written != expected => {
                        return Err(input_error("stdin close sequence failed"));
                    }
                    Some(InputFailure::Stopped | InputFailure::BrokenPipe) => {}
                    None => {
                        if platform_close {
                            process.graceful_stop(crate::GracefulAction::CloseInput)?;
                        }
                    }
                }
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => return Ok(true),
        }
    }
    Ok(false)
}

pub(super) fn drain_controls(
    receiver: &Receiver<ControlCommand>,
    process: &mut dyn PlatformProcess,
    plan: &SupervisorPlan,
    shared: &Arc<SharedObservation>,
) -> Result<(), ProcessError> {
    for _ in 0..IO_DRAIN_BUDGET {
        match receiver.try_recv() {
            Ok(ControlCommand::Resize(size)) => {
                process.resize(size)?;
                emit(shared, plan, None, ProcessEventKind::Resized(size), Vec::new());
            }
            Ok(ControlCommand::Signal(signal)) => {
                let action = match signal {
                    crate::ProcessSignal::Interrupt => crate::GracefulAction::Interrupt,
                    crate::ProcessSignal::Terminate => crate::GracefulAction::Terminate,
                };
                process.graceful_stop(action)?;
                emit(shared, plan, None, ProcessEventKind::Signalled(signal), Vec::new());
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drain_output(
    receiver: &Receiver<ReaderMessage>,
    eof_count: &mut usize,
    reader_failed: &mut bool,
    accounting: &mut AccountingSet,
    spools: &mut SpoolSet,
    total_spooled: &mut u64,
    window: &mut RetainedWindow,
    plan: &SupervisorPlan,
    shared: &Arc<SharedObservation>,
    store: &ProcessStore,
    lifecycle: &mut LifecycleState,
    stopping_at: &mut Option<Instant>,
    graceful_attempted: &mut bool,
    forced: &mut bool,
    process: &mut dyn PlatformProcess,
    input: &InputTask,
    native: &mut Option<Box<dyn crate::NativeSandboxSession>>,
) -> Result<(), ProcessError> {
    for _ in 0..IO_DRAIN_BUDGET {
        match receiver.try_recv() {
            Ok(ReaderMessage::Data(stream, bytes)) => {
                accept_output(
                    stream,
                    &bytes,
                    accounting,
                    spools,
                    total_spooled,
                    window,
                    plan,
                    shared,
                )?;
                if accounting.get_mut(stream).exceeded()
                    && plan.output_policy().overflow_action() == OutputOverflowAction::Terminate
                {
                    accept_trigger(
                        CancellationReason::OutputLimit,
                        plan,
                        shared,
                        store,
                        lifecycle,
                        stopping_at,
                        graceful_attempted,
                        forced,
                        process,
                        input,
                        native,
                    )?;
                }
            }
            Ok(ReaderMessage::Eof) => *eof_count = eof_count.saturating_add(1),
            Ok(ReaderMessage::Failed) => {
                *eof_count = eof_count.saturating_add(1);
                *reader_failed = true;
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn accept_output(
    stream: OutputStream,
    bytes: &[u8],
    accounting: &mut AccountingSet,
    spools: &mut SpoolSet,
    total_spooled: &mut u64,
    window: &mut RetainedWindow,
    plan: &SupervisorPlan,
    shared: &Arc<SharedObservation>,
) -> Result<(), ProcessError> {
    let account = accounting.get_mut(stream);
    let offset = account.observed();
    let global_available = plan
        .output_policy()
        .spool_limit()
        .map(|ceiling| {
            ceiling
                .checked_sub(*total_spooled)
                .ok_or_else(|| supervisor_error("aggregate spool accounting exceeds its allowance"))
        })
        .transpose()?;
    let reservation = account.reserve(bytes.len(), global_available)?;
    let accepted = reservation.accepted();
    let (retained, write_failed) = if accepted == 0 {
        (0, false)
    } else {
        match spool_mut(spools, stream) {
            Ok(spool) => match spool.write(&bytes[..accepted]) {
                Ok(written) => (written, false),
                Err(failure) => {
                    let written = failure.written();
                    let _original_cause = failure.into_error();
                    (written, true)
                }
            },
            Err(_original_cause) => (0, true),
        }
    };
    account.commit(reservation, retained)?;
    if write_failed {
        account.fail();
    }
    if retained > 0 {
        *total_spooled = total_spooled
            .checked_add(retained)
            .ok_or_else(|| supervisor_error("aggregate spool accounting overflowed"))?;
    }
    // The durable spool is the exact retained prefix, while the bounded live window and event
    // stream are observations. Continue advancing both with every byte read from the OS even
    // after the archival allowance is exhausted so late diagnostics replace the old live tail.
    window.push(stream, bytes);
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    state.retained_stdout = window.stream_bytes(OutputStream::Stdout);
    state.retained_stderr = window.stream_bytes(OutputStream::Stderr);
    state.retained_terminal = window.stream_bytes(OutputStream::Terminal);
    drop(state);
    emit(shared, plan, Some(offset), ProcessEventKind::Output(stream), bytes.to_vec());
    // Archive pressure fixes this stream as incomplete. The configured overflow action then
    // either accepts continued live observation or records the ordinary output-limit trigger.
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn accept_trigger(
    reason: CancellationReason,
    plan: &SupervisorPlan,
    shared: &Arc<SharedObservation>,
    store: &ProcessStore,
    lifecycle: &mut LifecycleState,
    stopping_at: &mut Option<Instant>,
    graceful_attempted: &mut bool,
    forced: &mut bool,
    process: &mut dyn PlatformProcess,
    input: &InputTask,
    native: &mut Option<Box<dyn crate::NativeSandboxSession>>,
) -> Result<(), ProcessError> {
    if lifecycle.first_trigger().is_some()
        || !matches!(lifecycle.phase(), LifecyclePhase::Starting | LifecyclePhase::Running)
    {
        return Ok(());
    }
    let sequence = emit(shared, plan, None, ProcessEventKind::Cancellation(reason), Vec::new());
    if lifecycle.request_stop(sequence, reason) {
        let trigger = lifecycle.first_trigger().expect("first trigger was just accepted");
        store.record_stopping(plan.process_id(), trigger)?;
        if let Some(session) = native.as_deref_mut() {
            let cancellation = session.cancellation_requested(reason);
            let capture = crate::native::capture_activated_session(
                store,
                session,
                plan,
                plan.sandbox_digest(),
            );
            cancellation?;
            capture?;
            super::publish_native_recovery(shared, session)?;
        }
        match plan.deadline_policy().stop_strategy() {
            StopStrategy::Force => {
                process.force_kill()?;
                *forced = true;
                emit(shared, plan, None, ProcessEventKind::Escalated, Vec::new());
            }
            StopStrategy::GracefulThenForce { action, .. } => {
                process.graceful_stop(action)?;
                *graceful_attempted = true;
            }
        }
        input.request_stop()?;
        *stopping_at = Some(Instant::now());
    }
    Ok(())
}

pub(super) struct AccountingSet {
    stdout: Option<OutputAccounting>,
    stderr: Option<OutputAccounting>,
    terminal: Option<OutputAccounting>,
}

impl AccountingSet {
    pub(super) const fn new(policy: OutputPolicy, mode: crate::IoMode) -> Self {
        match mode {
            crate::IoMode::Pipes => Self {
                stdout: Some(OutputAccounting::new(OutputStream::Stdout, policy.stdout_limit())),
                stderr: Some(OutputAccounting::new(OutputStream::Stderr, policy.stderr_limit())),
                terminal: None,
            },
            crate::IoMode::Pty(_) => Self {
                stdout: None,
                stderr: None,
                terminal: Some(OutputAccounting::new(
                    OutputStream::Terminal,
                    policy.terminal_limit(),
                )),
            },
        }
    }

    const fn get_mut(&mut self, stream: OutputStream) -> &mut OutputAccounting {
        match stream {
            OutputStream::Stdout => self.stdout.as_mut(),
            OutputStream::Stderr => self.stderr.as_mut(),
            OutputStream::Terminal => self.terminal.as_mut(),
        }
        .expect("platform emitted only the stream selected by the execution plan")
    }

    pub(super) fn fail_all(&mut self) {
        for accounting in
            [&mut self.stdout, &mut self.stderr, &mut self.terminal].into_iter().flatten()
        {
            accounting.fail();
        }
    }

    pub(super) fn finish(self) -> Vec<StreamAccounting> {
        self.stdout
            .into_iter()
            .chain(self.stderr)
            .chain(self.terminal)
            .map(OutputAccounting::finish)
            .collect()
    }
}

fn spool_mut(
    spools: &mut SpoolSet,
    stream: OutputStream,
) -> Result<&mut crate::output::SegmentedSpool, ProcessError> {
    match stream {
        OutputStream::Stdout => spools.stdout.as_mut(),
        OutputStream::Stderr => spools.stderr.as_mut(),
        OutputStream::Terminal => spools.terminal.as_mut(),
    }
    .ok_or_else(|| supervisor_error("platform emitted an unconfigured output stream"))
}

pub(super) fn synchronize_spools(spools: &mut SpoolSet, failed: &mut bool) {
    for spool in
        [&mut spools.stdout, &mut spools.stderr, &mut spools.terminal].into_iter().flatten()
    {
        if spool.synchronize().is_err() {
            *failed = true;
        }
    }
}

pub(super) fn join_readers(tasks: Vec<JoinHandle<()>>, failed: &mut bool) -> bool {
    let mut joined = true;
    for task in tasks {
        if task.join().is_err() {
            *failed = true;
            joined = false;
        }
    }
    joined
}

const fn input_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Input,
        crate::ProcessOperation::Stream,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}
