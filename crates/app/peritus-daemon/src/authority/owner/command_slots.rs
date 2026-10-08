//! Bounded active command ownership separate from immutable journal receipts.

use std::collections::BTreeMap;

use peritus_journal::{ApplicationCommandRecord, ApplicationCommandState};
use peritus_types::{CommandId, SessionId};

use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) struct ActiveCommandSlots {
    maximum: usize,
    commands: BTreeMap<CommandId, SessionId>,
    sessions: BTreeMap<SessionId, usize>,
}

impl ActiveCommandSlots {
    pub(super) const fn new(maximum: usize) -> Self {
        Self { maximum, commands: BTreeMap::new(), sessions: BTreeMap::new() }
    }

    pub(super) fn can_claim(
        &self,
        command_id: CommandId,
        session_id: SessionId,
        session_maximum: usize,
    ) -> Result<bool, DaemonError> {
        if session_maximum == 0 {
            return Err(invalid_limit());
        }
        if let Some(owner) = self.commands.get(&command_id) {
            return if *owner == session_id { Ok(true) } else { Err(identity_conflict()) };
        }
        let session_maximum = session_maximum.min(self.maximum);
        let session_count = self.sessions.get(&session_id).copied().unwrap_or(0);
        Ok(self.commands.len() < self.maximum && session_count < session_maximum)
    }

    pub(super) fn claim(
        &mut self,
        record: &ApplicationCommandRecord,
        session_maximum: usize,
    ) -> Result<(), DaemonError> {
        if self.can_claim(record.command_id(), record.session_id(), session_maximum)? {
            if self.commands.contains_key(&record.command_id()) {
                return Ok(());
            }
            self.commands.insert(record.command_id(), record.session_id());
            let count = self.sessions.entry(record.session_id()).or_default();
            *count = count.checked_add(1).ok_or_else(identity_conflict)?;
            Ok(())
        } else {
            Err(capacity())
        }
    }

    pub(super) fn release_if_terminal(&mut self, record: &ApplicationCommandRecord) {
        if !matches!(
            record.state(),
            ApplicationCommandState::Committed | ApplicationCommandState::Rejected
        ) {
            return;
        }
        let Some(session_id) = self.commands.remove(&record.command_id()) else { return };
        let Some(count) = self.sessions.get_mut(&session_id) else { return };
        if *count <= 1 {
            self.sessions.remove(&session_id);
        } else {
            *count -= 1;
        }
    }
}

pub(super) fn capacity() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::ResourceLimit,
        DaemonRecovery::Retry,
        "admit application command",
        "all active idempotency slots are occupied",
    )
}

fn invalid_limit() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "admit application command",
        "active idempotency slot limit is zero",
    )
}

fn identity_conflict() -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::CorruptState,
        DaemonRecovery::Operator,
        "account for active application command",
        "active command identity or session count is inconsistent",
    )
}
