//! Deterministic local prompt-view proposals over source-bound immutable public replies.

use super::{ControlStore, ControlStoreError as Error};
use peritus_app_protocol::{
    WorkbenchCompactionEntry, WorkbenchCompactionPreview, WorkbenchCompactionRequest,
    WorkbenchInvocationId,
};
use peritus_product_runner::control::{
    ContextPreference, ContextTarget, ControlError, ConversationId, PromptView,
};
use peritus_types::ActorId;

const RECENT_COMPLETE_REPLIES: usize = 2;

impl ControlStore {
    pub(crate) fn compaction_run(
        &self,
        actor: ActorId,
        request: &WorkbenchCompactionRequest,
    ) -> Result<peritus_types::RunId, Error> {
        let record = self.compaction_record(actor, request)?;
        let execution = record.execution().ok_or(ControlError::InvalidInput)?;
        peritus_types::RunId::new(*execution.run_bytes())
            .map_err(|_| ControlError::InvalidInput.into())
    }

    pub(crate) fn compaction_preview(
        &self,
        actor: ActorId,
        request: &WorkbenchCompactionRequest,
        execution_complete: bool,
    ) -> Result<(Option<PromptView>, WorkbenchCompactionPreview), Error> {
        let record = self.compaction_record(actor, request)?;
        let generation =
            record.prompt_view().generation().checked_add(1).ok_or(ControlError::Capacity)?;
        let metadata = record
            .replies()
            .iter()
            .map(|reply| (reply.after_invocation(), String::new()))
            .collect();
        let capture = record.capture_with_replies(&metadata, true)?;
        let (pinned, mut candidates): (Vec<_>, Vec<_>) = record
            .replies()
            .iter()
            .filter(|reply| capture.public_replies().contains(&reply.after_invocation()))
            .partition(|reply| {
                record.context().preference(ContextTarget::PublicReply(reply.after_invocation()))
                    == Some(ContextPreference::Pinned)
            });
        if !execution_complete {
            let preview = WorkbenchCompactionPreview::new(
                request.clone(),
                generation,
                0,
                count(pinned.len())?,
                count(candidates.len())?,
                0,
                Vec::new(),
            )
            .map_err(|_| ControlError::InvalidInput)?;
            return Ok((None, preview));
        }
        let recent = candidates.len().min(RECENT_COMPLETE_REPLIES);
        candidates.truncate(candidates.len().saturating_sub(recent));
        let focus = request.focus().map(peritus_app_protocol::WorkbenchCompactionFocus::as_str);
        let mut reducible = Vec::new();
        let mut unsavable = 0_usize;
        for source in candidates {
            if PromptView::source_is_reducible(source, focus)? {
                reducible.push((source.clone(), self.reply_text(source)?));
            } else {
                unsavable = unsavable.checked_add(1).ok_or(ControlError::Capacity)?;
            }
        }
        let view = if reducible.is_empty() {
            None
        } else {
            Some(PromptView::compact(generation, focus.map(str::to_owned), &reducible)?)
        };
        let entries = view
            .as_ref()
            .map_or(&[][..], PromptView::entries)
            .iter()
            .map(|entry| {
                WorkbenchCompactionEntry::new(
                    WorkbenchInvocationId::new(*entry.invocation().as_bytes())
                        .map_err(|_| ControlError::InvalidInput)?,
                    entry.source_digest(),
                    entry.source_bytes(),
                    peritus_codec::sha256(entry.replacement().as_bytes()),
                    entry.replacement().len() as u64,
                )
                .map_err(|_| ControlError::InvalidInput.into())
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let preview = WorkbenchCompactionPreview::new(
            request.clone(),
            generation,
            count(recent)?,
            count(pinned.len())?,
            0,
            count(unsavable)?,
            entries,
        )
        .map_err(|_| ControlError::InvalidInput)?;
        Ok((view, preview))
    }

    fn compaction_record(
        &self,
        actor: ActorId,
        request: &WorkbenchCompactionRequest,
    ) -> Result<peritus_product_runner::control::ConversationRecord, Error> {
        let scope = request.query();
        let id = ConversationId::new(scope.conversation().into_bytes())?;
        let record = self.load(id)?.ok_or(ControlError::NotFound)?;
        if record.owner_bytes() != actor.as_bytes()
            || record.workspace_bytes() != scope.workspace().as_bytes()
        {
            return Err(ControlError::ScopeMismatch.into());
        }
        if record.revision() != request.revision() {
            return Err(ControlError::StaleRevision.into());
        }
        Ok(record)
    }
}

fn count(value: usize) -> Result<u32, Error> {
    u32::try_from(value).map_err(|_| ControlError::Capacity.into())
}
