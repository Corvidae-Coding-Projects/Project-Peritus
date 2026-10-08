//! Bounded ordered stdin admission separated from the process owner.

use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use crate::{ControlRejection, ErrorCode, ProcessError, ProcessOperation, RecoveryClass, StdinPolicy};

const INPUT_QUEUE: usize = 64;
const WAIT_POLL_MILLIS: u64 = 1;

#[derive(Clone)]
pub(crate) struct InputLane {
    inner: Arc<InputShared>,
    policy: StdinPolicy,
}

pub(crate) struct InputOwner {
    inner: Arc<InputShared>,
}

pub(crate) enum InputCommand {
    Write(Vec<u8>),
    Close,
}

struct InputShared {
    state: Mutex<InputState>,
    changed: Condvar,
}

struct InputState {
    queue: VecDeque<InputCommand>,
    accepting: bool,
    owner_live: bool,
    started: bool,
    stop_requested: bool,
    reserved: u64,
    active: u64,
    acknowledged: u64,
}

pub(crate) fn lane(policy: StdinPolicy) -> (InputLane, InputOwner) {
    let inner = Arc::new(InputShared {
        state: Mutex::new(InputState {
            queue: VecDeque::with_capacity(INPUT_QUEUE),
            accepting: !matches!(policy, StdinPolicy::Closed),
            owner_live: true,
            started: false,
            stop_requested: false,
            reserved: 0,
            active: 0,
            acknowledged: 0,
        }),
        changed: Condvar::new(),
    });
    (
        InputLane { inner: Arc::clone(&inner), policy },
        InputOwner { inner },
    )
}

impl InputLane {
    pub(crate) fn write(&self, bytes: Vec<u8>) -> Result<(), ProcessError> {
        let length = self.validate_write(&bytes)?;
        let mut state = self.lock();
        validate_open(&state)?;
        validate_total(&state, self.policy, length)?;
        if state.queue.len() == INPUT_QUEUE {
            return Err(queue_full());
        }
        accept_write(&mut state, bytes, length)?;
        drop(state);
        self.inner.changed.notify_one();
        Ok(())
    }

    pub(crate) fn write_blocking(&self, bytes: Vec<u8>) -> Result<(), ProcessError> {
        let length = self.validate_write(&bytes)?;
        let mut state = self.lock();
        loop {
            validate_open(&state)?;
            validate_total(&state, self.policy, length)?;
            if state.queue.len() < INPUT_QUEUE {
                accept_write(&mut state, bytes, length)?;
                drop(state);
                self.inner.changed.notify_one();
                return Ok(());
            }
            state = self
                .inner
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn write_while(
        &self,
        bytes: Vec<u8>,
        mut keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        let length = self.validate_write(&bytes)?;
        let mut bytes = Some(bytes);
        loop {
            if !keep_waiting() {
                return Ok(false);
            }
            let mut state = self.lock();
            validate_open(&state)?;
            validate_total(&state, self.policy, length)?;
            if state.queue.len() < INPUT_QUEUE {
                accept_write(
                    &mut state,
                    bytes.take().expect("stdin bytes are consumed only after admission"),
                    length,
                )?;
                drop(state);
                self.inner.changed.notify_one();
                return Ok(true);
            }
            let (_state, _) = self
                .inner
                .changed
                .wait_timeout(state, Duration::from_millis(WAIT_POLL_MILLIS))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn close(&self) -> Result<(), ProcessError> {
        let mut state = self.lock();
        validate_owner(&state)?;
        if !state.accepting {
            return Ok(());
        }
        if state.queue.len() == INPUT_QUEUE {
            return Err(queue_full());
        }
        accept_close(&mut state);
        drop(state);
        self.inner.changed.notify_one();
        Ok(())
    }

    pub(crate) fn close_blocking(&self) -> Result<(), ProcessError> {
        let mut state = self.lock();
        loop {
            validate_owner(&state)?;
            if !state.accepting {
                return Ok(());
            }
            if state.queue.len() < INPUT_QUEUE {
                accept_close(&mut state);
                drop(state);
                self.inner.changed.notify_one();
                return Ok(());
            }
            state = self
                .inner
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn close_while(
        &self,
        mut keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        loop {
            if !keep_waiting() {
                return Ok(false);
            }
            let mut state = self.lock();
            validate_owner(&state)?;
            if !state.accepting {
                return Ok(true);
            }
            if state.queue.len() < INPUT_QUEUE {
                accept_close(&mut state);
                drop(state);
                self.inner.changed.notify_one();
                return Ok(true);
            }
            let (_state, _) = self
                .inner
                .changed
                .wait_timeout(state, Duration::from_millis(WAIT_POLL_MILLIS))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn request_stop(&self) {
        let mut state = self.lock();
        state.accepting = false;
        state.stop_requested = true;
        discard_queue(&mut state);
        drop(state);
        self.inner.changed.notify_all();
    }

    pub(crate) fn activate(&self) {
        let mut state = self.lock();
        state.started = true;
        drop(state);
        self.inner.changed.notify_all();
    }

    fn validate_write(&self, bytes: &[u8]) -> Result<u64, ProcessError> {
        let max_write_bytes = match self.policy {
            StdinPolicy::Closed => return Err(input_error("stdin is disabled for this process")),
            StdinPolicy::Bounded { max_write_bytes, .. }
            | StdinPolicy::Streaming { max_write_bytes } => max_write_bytes,
        };
        let length = u64::try_from(bytes.len())
            .map_err(|_| input_error("stdin write length is unrepresentable"))?;
        if length == 0 || length > max_write_bytes {
            return Err(input_error("stdin write is empty or exceeds its per-write bound"));
        }
        Ok(length)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InputState> {
        self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl InputOwner {
    pub(crate) fn next(&self) -> Option<InputCommand> {
        let mut state = self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if state.stop_requested {
                return None;
            }
            if state.started
                && let Some(command) = state.queue.pop_front()
            {
                if let InputCommand::Write(bytes) = &command {
                    state.active = u64::try_from(bytes.len())
                        .expect("accepted stdin length remains representable");
                }
                self.inner.changed.notify_all();
                return Some(command);
            }
            state = self
                .inner
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn finish_write(&self, requested: u64, written: u64, terminal: bool) {
        let mut state = self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        debug_assert_eq!(state.active, requested);
        state.active = 0;
        state.reserved = state
            .reserved
            .checked_sub(requested)
            .expect("active stdin reservation was admitted exactly once");
        state.acknowledged = state
            .acknowledged
            .checked_add(written)
            .expect("acknowledged stdin cannot exceed admitted accounting");
        if terminal {
            state.accepting = false;
            state.stop_requested = true;
            discard_queue(&mut state);
        }
        drop(state);
        self.inner.changed.notify_all();
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop_requested
    }

    pub(crate) fn wait_for_retry(&self) -> bool {
        let state = self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stop_requested {
            return false;
        }
        let (state, _) = self
            .inner
            .changed
            .wait_timeout(state, Duration::from_millis(WAIT_POLL_MILLIS))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !state.stop_requested
    }

    pub(crate) fn finish(&self) {
        let mut state = self.inner.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.accepting = false;
        state.owner_live = false;
        state.stop_requested = true;
        discard_queue(&mut state);
        drop(state);
        self.inner.changed.notify_all();
    }
}

impl Drop for InputOwner {
    fn drop(&mut self) {
        self.finish();
    }
}

fn validate_owner(state: &InputState) -> Result<(), ProcessError> {
    if state.owner_live {
        Ok(())
    } else {
        Err(owner_closed())
    }
}

fn validate_open(state: &InputState) -> Result<(), ProcessError> {
    validate_owner(state)?;
    if state.accepting {
        Ok(())
    } else {
        Err(owner_closed())
    }
}

fn validate_total(
    state: &InputState,
    policy: StdinPolicy,
    length: u64,
) -> Result<(), ProcessError> {
    let Some(maximum) = (match policy {
        StdinPolicy::Bounded { max_total_bytes, .. } => Some(max_total_bytes),
        StdinPolicy::Closed => Some(0),
        StdinPolicy::Streaming { .. } => None,
    }) else {
        return Ok(());
    };
    let attempted = state
        .acknowledged
        .checked_add(state.reserved)
        .and_then(|value| value.checked_add(length))
        .ok_or_else(|| input_error("stdin total accounting overflowed"))?;
    if attempted > maximum {
        Err(input_error("stdin cumulative bound exceeded"))
    } else {
        Ok(())
    }
}

fn accept_write(
    state: &mut InputState,
    bytes: Vec<u8>,
    length: u64,
) -> Result<(), ProcessError> {
    state.reserved = state
        .reserved
        .checked_add(length)
        .ok_or_else(|| input_error("stdin total accounting overflowed"))?;
    state.queue.push_back(InputCommand::Write(bytes));
    Ok(())
}

fn accept_close(state: &mut InputState) {
    state.accepting = false;
    state.queue.push_back(InputCommand::Close);
}

fn discard_queue(state: &mut InputState) {
    while let Some(command) = state.queue.pop_front() {
        if let InputCommand::Write(bytes) = command {
            let length = u64::try_from(bytes.len()).expect("accepted stdin length remains valid");
            state.reserved = state
                .reserved
                .checked_sub(length)
                .expect("queued stdin reservation was admitted exactly once");
        }
    }
}

const fn queue_full() -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::CorrectRequest,
        "bounded process input queue is full",
    )
    .rejecting_control(ControlRejection::Backpressure)
}

const fn input_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::CorrectRequest,
        detail,
    )
    .rejecting_control(ControlRejection::InvalidRequest)
}

const fn owner_closed() -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::Terminal,
        "process input is closed",
    )
    .rejecting_control(ControlRejection::AdmissionClosed)
}
