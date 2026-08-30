//! Runtime-neutral future and worker for one persistent controller process.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::thread::{self, JoinHandle};

use crate::{CancellationToken, SubjectError, SubjectErrorCode};

use super::process::OwnedController;
use super::protocol::{EncodedRequest, Stage};
use super::{NativeControllerLimits, subject_error};

pub(super) use super::process::LaunchRequest;

pub(super) struct ControllerHandle {
    sender: Sender<WorkerMessage>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ControllerHandle {
    pub(super) fn launch(
        request: LaunchRequest<'_>,
        limits: NativeControllerLimits,
        cancellation: CancellationToken,
    ) -> Result<Self, SubjectError> {
        let controller = OwnedController::launch(request, limits)?;
        let (sender, receiver) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("peritus-h1-controller".to_owned())
            .spawn(move || {
                run_worker(controller, &receiver, &worker_stop, &cancellation, limits);
            })
            .map_err(|error| supervision(format!("start controller worker: {error}"), false))?;
        Ok(Self { sender, stop, worker: Some(worker) })
    }

    pub(super) fn command(&self, stage: Stage, request: EncodedRequest) -> CommandFuture {
        let response_state = Arc::new(ResponseState::new());
        if self
            .sender
            .send(WorkerMessage::Execute { stage, request, state: Arc::clone(&response_state) })
            .is_err()
        {
            response_state.complete(Err(supervision(
                "native controller worker is no longer available",
                false,
            )));
        }
        CommandFuture { state: response_state, completed: false }
    }

    pub(super) fn finish(&mut self) -> Result<(), SubjectError> {
        join_worker(self.worker.take())
    }
}

impl Drop for ControllerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.sender.send(WorkerMessage::Stop);
        let _ = join_worker(self.worker.take());
    }
}

pub(super) struct CommandFuture {
    state: Arc<ResponseState>,
    completed: bool,
}

impl Future for CommandFuture {
    type Output = Result<Vec<u8>, SubjectError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let result = lock(&self.state.result).take();
        if let Some(result) = result {
            self.completed = true;
            return Poll::Ready(result);
        }
        *lock(&self.state.waker) = Some(context.waker().clone());
        let result = lock(&self.state.result).take();
        if let Some(result) = result {
            lock(&self.state.waker).take();
            self.completed = true;
            return Poll::Ready(result);
        }
        Poll::Pending
    }
}

impl Drop for CommandFuture {
    fn drop(&mut self) {
        if !self.completed {
            self.state.abandoned.store(true, Ordering::Release);
        }
    }
}

enum WorkerMessage {
    Execute { stage: Stage, request: EncodedRequest, state: Arc<ResponseState> },
    Stop,
}

struct ResponseState {
    result: Mutex<Option<Result<Vec<u8>, SubjectError>>>,
    waker: Mutex<Option<Waker>>,
    abandoned: AtomicBool,
}

impl ResponseState {
    const fn new() -> Self {
        Self {
            result: Mutex::new(None),
            waker: Mutex::new(None),
            abandoned: AtomicBool::new(false),
        }
    }

    fn complete(&self, result: Result<Vec<u8>, SubjectError>) {
        *lock(&self.result) = Some(result);
        let waker = lock(&self.waker).take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

fn run_worker(
    mut controller: OwnedController,
    receiver: &Receiver<WorkerMessage>,
    stop: &AtomicBool,
    cancellation: &CancellationToken,
    limits: NativeControllerLimits,
) {
    while let Ok(message) = receiver.recv() {
        match message {
            WorkerMessage::Execute { stage, request, state } => {
                let result = controller.exchange(
                    &request.bytes,
                    &state.abandoned,
                    stop,
                    cancellation,
                    limits,
                    stage == Stage::Cleanup,
                );
                let terminal = result.is_err() || stage == Stage::Cleanup;
                state.complete(result);
                if terminal {
                    return;
                }
            }
            WorkerMessage::Stop => return,
        }
    }
}

fn join_worker(worker: Option<JoinHandle<()>>) -> Result<(), SubjectError> {
    if let Some(worker) = worker {
        worker.join().map_err(|_| supervision("native controller worker panicked", false))?;
    }
    Ok(())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn supervision(detail: impl Into<String>, retryable: bool) -> SubjectError {
    subject_error(SubjectErrorCode::Supervision, detail, retryable)
}
