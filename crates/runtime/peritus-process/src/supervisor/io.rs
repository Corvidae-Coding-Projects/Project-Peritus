//! Bounded process input, output-reader tasks, and spool accounting.

use std::{
    io::{ErrorKind, Read, Write},
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use crate::{
    CancellationReason, ExecutionPlan, LifecyclePhase, LifecycleState, OutputOverflowAction,
    OutputPolicy, OutputStream, ProcessError, ProcessEventKind, ProcessStore, StdinPolicy,
    StreamAccounting,
    control::{ControlCommand, SharedObservation},
    output::{OutputAccounting, RetainedWindow, SpoolSet},
    platform::{self, PlatformProcess},
};

use super::{emit, supervisor_error};

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
            Err(_) => {
                let _ = sender.send(ReaderMessage::Failed);
                break;
            }
        }
    }
    drop(sender);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drain_controls(
    receiver: &Receiver<ControlCommand>,
    process: &mut dyn PlatformProcess,
    input: &mut Option<Box<dyn Write + Send>>,
    input_written: &mut u64,
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
    store: &ProcessStore,
    lifecycle: &mut LifecycleState,
    stopping_at: &mut Option<Instant>,
    graceful_attempted: &mut bool,
) -> Result<(), ProcessError> {
    loop {
        match receiver.try_recv() {
            Ok(ControlCommand::Write(bytes)) => {
                write_input(&bytes, input, input_written, plan, shared)?;
            }
            Ok(ControlCommand::CloseInput) => {
                if input.take().is_some() {
                    emit(shared, plan, None, ProcessEventKind::StdinClosed, Vec::new());
                }
            }
            Ok(ControlCommand::Resize(size)) => {
                process.resize(size)?;
                emit(shared, plan, None, ProcessEventKind::Resized(size), Vec::new());
            }
            Ok(ControlCommand::Cancel(reason)) => accept_trigger(
                reason,
                plan,
                shared,
                store,
                lifecycle,
                stopping_at,
                graceful_attempted,
                process,
                input,
            )?,
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
    Ok(())
}

fn write_input(
    bytes: &[u8],
    input: &mut Option<Box<dyn Write + Send>>,
    input_written: &mut u64,
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
) -> Result<(), ProcessError> {
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let total_limit = match plan.stdin_policy() {
        StdinPolicy::Bounded { max_total_bytes, .. } => max_total_bytes,
        StdinPolicy::Closed => 0,
    };
    let attempted = input_written
        .checked_add(length)
        .ok_or_else(|| input_error("stdin total accounting overflowed"))?;
    if attempted > total_limit {
        return Err(input_error("stdin cumulative bound exceeded"));
    }
    let writer = input.as_mut().ok_or_else(|| input_error("stdin is already closed"))?;
    match writer.write_all(bytes).and_then(|()| writer.flush()) {
        Ok(()) => {
            *input_written = attempted;
            emit(shared, plan, None, ProcessEventKind::StdinAccepted { bytes: length }, Vec::new());
        }
        Err(error) if error.kind() == ErrorKind::BrokenPipe => {
            input.take();
            emit(shared, plan, None, ProcessEventKind::StdinClosed, Vec::new());
        }
        Err(_) => return Err(input_error("stdin write failed")),
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
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
    store: &ProcessStore,
    lifecycle: &mut LifecycleState,
    stopping_at: &mut Option<Instant>,
    graceful_attempted: &mut bool,
    process: &mut dyn PlatformProcess,
    input: &mut Option<Box<dyn Write + Send>>,
) -> Result<(), ProcessError> {
    loop {
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
                        process,
                        input,
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
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
) -> Result<(), ProcessError> {
    let account = accounting.get_mut(stream);
    let offset = account.observed();
    let global_available = plan.output_policy().spool_bytes().saturating_sub(*total_spooled);
    let accepted = account.observe(bytes.len(), global_available);
    if accepted == 0 {
        return Ok(());
    }
    spool_mut(spools, stream)?.write(&bytes[..accepted])?;
    *total_spooled = total_spooled.saturating_add(u64::try_from(accepted).unwrap_or(u64::MAX));
    window.push(&bytes[..accepted]);
    let retained = window.bytes();
    shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retained_output =
        retained;
    emit(shared, plan, Some(offset), ProcessEventKind::Output(stream), bytes[..accepted].to_vec());
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn accept_trigger(
    reason: CancellationReason,
    plan: &ExecutionPlan,
    shared: &Arc<SharedObservation>,
    store: &ProcessStore,
    lifecycle: &mut LifecycleState,
    stopping_at: &mut Option<Instant>,
    graceful_attempted: &mut bool,
    process: &mut dyn PlatformProcess,
    input: &mut Option<Box<dyn Write + Send>>,
) -> Result<(), ProcessError> {
    if lifecycle.first_trigger().is_some()
        || !matches!(lifecycle.phase(), LifecyclePhase::Starting | LifecyclePhase::Running)
    {
        return Ok(());
    }
    let sequence = emit(shared, plan, None, ProcessEventKind::Cancellation(reason), Vec::new());
    if lifecycle.request_stop(sequence, reason) {
        let trigger = lifecycle.first_trigger().expect("first trigger was just accepted");
        store.record_stopping(plan.identity().process_id(), trigger)?;
        input.take();
        process.graceful_stop(plan.deadline_policy().graceful_action())?;
        *graceful_attempted = true;
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
                stdout: Some(OutputAccounting::new(OutputStream::Stdout, policy.stdout_bytes())),
                stderr: Some(OutputAccounting::new(OutputStream::Stderr, policy.stderr_bytes())),
                terminal: None,
            },
            crate::IoMode::Pty(_) => Self {
                stdout: None,
                stderr: None,
                terminal: Some(OutputAccounting::new(
                    OutputStream::Terminal,
                    policy.terminal_bytes(),
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
) -> Result<&mut crate::output::BoundedSpool, ProcessError> {
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
