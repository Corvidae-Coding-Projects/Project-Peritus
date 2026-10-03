//! Preserve send order while polling writer backpressure beside terminal input.

use crate::{TuiError, client::ClientSession};
use peritus_app_protocol::AppMessage;
use std::{collections::VecDeque, future::Future, pin::Pin};

const MAX_PENDING_SENDS: usize = 128;
type Send = Pin<Box<dyn Future<Output = Result<(), TuiError>> + std::marker::Send>>;

#[derive(Default)]
pub(super) struct Outbox {
    pending: VecDeque<AppMessage>,
    sending: Option<Send>,
}

impl Outbox {
    pub(super) fn push(
        &mut self,
        session: &ClientSession,
        message: AppMessage,
    ) -> Result<(), TuiError> {
        if self.sending.is_none() {
            self.sending = Some(Box::pin(session.send(message)));
        } else if self.pending.len() < MAX_PENDING_SENDS {
            self.pending.push_back(message);
        } else {
            return Err(TuiError::Task("daemon is not accepting requests; pending drafts were retained for inspection before reconnecting".into()));
        }
        Ok(())
    }

    pub(super) const fn active(&self) -> bool {
        self.sending.is_some()
    }

    pub(super) async fn next(&mut self) -> Result<(), TuiError> {
        let Some(sending) = self.sending.as_mut() else {
            return Err(TuiError::Task("daemon send is not active".into()));
        };
        let result = sending.await;
        self.sending = None;
        result
    }

    pub(super) fn advance(&mut self, session: &ClientSession) {
        if self.sending.is_none()
            && let Some(message) = self.pending.pop_front()
        {
            self.sending = Some(Box::pin(session.send(message)));
        }
    }
}
