//! Capacity-independent, first-reason cancellation admission.

use std::sync::{Arc, Mutex};

use crate::{
    CancellationReason, ErrorCode, ProcessError, ProcessOperation, RecoveryClass,
};

#[derive(Clone)]
pub(crate) struct CancellationLane {
    state: Arc<Mutex<CancellationState>>,
}

pub(crate) struct CancellationOwner {
    state: Arc<Mutex<CancellationState>>,
}

struct CancellationState {
    first: Option<CancellationReason>,
    delivered: bool,
    accepting: bool,
    owner_live: bool,
}

pub(crate) fn lane() -> (CancellationLane, CancellationOwner) {
    let state = Arc::new(Mutex::new(CancellationState {
        first: None,
        delivered: false,
        accepting: true,
        owner_live: true,
    }));
    (
        CancellationLane { state: Arc::clone(&state) },
        CancellationOwner { state },
    )
}

impl CancellationLane {
    pub(crate) fn request(&self, reason: CancellationReason) -> Result<(), ProcessError> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.owner_live || !state.accepting {
            return Err(owner_closed());
        }
        if state.first.is_none() {
            state.first = Some(reason);
        }
        Ok(())
    }

    pub(crate) fn request_while(
        &self,
        reason: CancellationReason,
        mut keep_waiting: impl FnMut() -> bool,
    ) -> Result<bool, ProcessError> {
        if !keep_waiting() {
            return Ok(false);
        }
        self.request(reason)?;
        Ok(true)
    }

    pub(crate) fn owner_finished(&self) -> bool {
        !self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).owner_live
    }
}

impl CancellationOwner {
    pub(crate) fn pending(&self) -> Option<CancellationReason> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first
    }

    pub(crate) fn take_pending(&self) -> Option<CancellationReason> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.delivered {
            None
        } else {
            state.delivered = state.first.is_some();
            state.first
        }
    }

    pub(crate) fn close_admission(&self) -> Option<CancellationReason> {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.accepting = false;
        if state.delivered {
            None
        } else {
            state.delivered = state.first.is_some();
            state.first
        }
    }

    fn retire(&self) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.accepting = false;
        state.owner_live = false;
    }
}

impl Drop for CancellationOwner {
    fn drop(&mut self) {
        self.retire();
    }
}

const fn owner_closed() -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::Terminal,
        "process owner has already terminated",
    )
}
