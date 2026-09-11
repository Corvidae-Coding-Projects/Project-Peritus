//! Pure successor planning. The caller publishes this successor and its receipt in one C0 append.

use super::{
    ControlError, InputId, InputLedger, InputRevision, InputSelection, InputState,
    InvocationInputs, MAX_DEPENDENCIES, MAX_INPUT_BYTES, MAX_INVOCATIONS, MAX_REVISIONS,
    QueueIntent,
};
use std::collections::BTreeSet;

impl InputLedger {
    /// Retains source context but requires explicit child unhold before pending work is actionable.
    pub(in crate::control) fn historical_snapshot(&self) -> Self {
        let mut next = self.clone();
        for item in &mut next.revisions {
            if item.state == InputState::Queued {
                item.state = InputState::Held;
            }
        }
        next
    }

    /// Advances the same effect fence for a non-message context-selection change.
    pub(in crate::control) fn context_changed(&self) -> Result<Self, ControlError> {
        self.validate()?;
        let mut next = self.clone();
        next.generation = next.generation.checked_add(1).ok_or(ControlError::Capacity)?;
        Ok(next)
    }

    /// Plans a bounded input transition without accepting it or executing a provider request.
    ///
    /// # Errors
    /// Rejects stale content, dependencies, malformed state, immutable-history edits, or capacity.
    pub fn apply(
        &self,
        author: peritus_types::ActorId,
        intent: &QueueIntent,
    ) -> Result<Self, ControlError> {
        self.validate()?;
        let mut next = self.clone();
        next.apply_inner(author.into_bytes(), intent)?;
        // Incorporation changes bookkeeping, not the governing instructions. Advancing this
        // fence for our own request admission would immediately invalidate its tool calls.
        if !matches!(intent, QueueIntent::Incorporate { .. }) {
            next.generation = self.generation.checked_add(1).ok_or(ControlError::Capacity)?;
        }
        next.validate()?;
        Ok(next)
    }

    fn apply_inner(&mut self, author: [u8; 16], intent: &QueueIntent) -> Result<(), ControlError> {
        match intent {
            QueueIntent::Enqueue { id, text, dependencies } => {
                self.enqueue(*id, author, text.clone(), dependencies.clone(), None)
            }
            QueueIntent::Correct { original, id, text } => {
                let item = self.exact(*original)?;
                if item.state != InputState::Incorporated {
                    return Err(ControlError::InvalidInput);
                }
                self.enqueue(*id, author, text.clone(), vec![original.id], Some(*original))
            }
            QueueIntent::Edit { selected, text } => {
                let item = self.pending_mut(*selected)?;
                let mut replacement = item.clone();
                replacement.selection.revision =
                    replacement.selection.revision.checked_add(1).ok_or(ControlError::Capacity)?;
                replacement.text = text.clone();
                replacement.author = author;
                item.state = InputState::Superseded;
                self.revisions.push(replacement);
                Ok(())
            }
            QueueIntent::Hold { selected, held } => {
                self.pending_mut(*selected)?.state =
                    if *held { InputState::Held } else { InputState::Queued };
                Ok(())
            }
            QueueIntent::Withdraw(selected) => {
                if self.order.iter().any(|id| {
                    self.latest(*id).is_some_and(|item| item.dependencies.contains(&selected.id))
                }) {
                    return Err(ControlError::InvalidInput);
                }
                self.pending_mut(*selected)?.state = InputState::Withdrawn;
                self.order.retain(|id| *id != selected.id);
                Ok(())
            }
            QueueIntent::Reorder(order) => {
                if order.len() != self.order.len()
                    || order.iter().copied().collect::<BTreeSet<_>>()
                        != self.order.iter().copied().collect()
                {
                    return Err(ControlError::InvalidInput);
                }
                self.order.clone_from(order);
                Ok(())
            }
            QueueIntent::Incorporate { invocation, request_digest, manifest_digest, items } => {
                if self.invocations.iter().any(|binding| binding.invocation == *invocation) {
                    return Err(ControlError::IdempotencyConflict);
                }
                let selected_ids: Vec<_> = items.iter().map(|selected| selected.id).collect();
                let current_order: Vec<_> =
                    self.order.iter().filter(|id| selected_ids.contains(id)).copied().collect();
                if current_order != selected_ids {
                    return Err(ControlError::InvalidInput);
                }
                for selected in items {
                    if self.pending_mut(*selected)?.state != InputState::Queued {
                        return Err(ControlError::InvalidInput);
                    }
                    let item = self.pending_mut(*selected)?;
                    item.state = InputState::Incorporated;
                    item.invocation = Some(*invocation);
                }
                self.order.retain(|id| !selected_ids.contains(id));
                self.invocations.push(InvocationInputs {
                    invocation: *invocation,
                    request_digest: *request_digest,
                    manifest_digest: *manifest_digest,
                    items: items.clone(),
                });
                Ok(())
            }
        }
    }

    fn enqueue(
        &mut self,
        id: InputId,
        author: [u8; 16],
        text: super::ControlText<8192>,
        dependencies: Vec<InputId>,
        correction_of: Option<InputSelection>,
    ) -> Result<(), ControlError> {
        if self.latest(id).is_some() {
            return Err(ControlError::IdempotencyConflict);
        }
        self.revisions.push(InputRevision {
            selection: InputSelection::new(id, 1)?,
            author,
            text,
            dependencies,
            state: InputState::Queued,
            correction_of,
            invocation: None,
        });
        self.order.push(id);
        Ok(())
    }

    fn exact(&self, selected: InputSelection) -> Result<&InputRevision, ControlError> {
        self.revisions
            .iter()
            .find(|item| item.selection == selected)
            .ok_or(ControlError::StaleRevision)
    }
    fn pending_mut(
        &mut self,
        selected: InputSelection,
    ) -> Result<&mut InputRevision, ControlError> {
        let item = self
            .revisions
            .iter_mut()
            .rev()
            .find(|item| item.selection.id == selected.id)
            .ok_or(ControlError::NotFound)?;
        if item.selection != selected {
            return Err(ControlError::StaleRevision);
        }
        if !matches!(item.state, InputState::Queued | InputState::Held) {
            return Err(ControlError::InvalidInput);
        }
        Ok(item)
    }

    pub(in crate::control) fn validate(&self) -> Result<(), ControlError> {
        if self.revisions.len() > MAX_REVISIONS
            || self.invocations.len() > MAX_INVOCATIONS
            || self.order.len() > MAX_REVISIONS
            || self.revisions.iter().map(|item| item.text.as_str().len()).sum::<usize>()
                > MAX_INPUT_BYTES
        {
            return Err(ControlError::Capacity);
        }
        if self.generation == 0 && !self.is_empty() {
            return Err(ControlError::InvalidInput);
        }
        for (index, item) in self.revisions.iter().enumerate() {
            self.validate_revision(index, item)?;
        }
        self.validate_order()?;
        self.validate_bindings()
    }

    fn validate_revision(&self, index: usize, item: &InputRevision) -> Result<(), ControlError> {
        let earlier = &self.revisions[..index];
        let previous = earlier.iter().rev().find(|prior| prior.selection.id == item.selection.id);
        if item.author == [0; 16]
            || item.selection.revision
                != previous
                    .map_or(Some(1), |prior| prior.selection.revision.checked_add(1))
                    .ok_or(ControlError::Capacity)?
            || previous.is_some_and(|prior| prior.state != InputState::Superseded)
            || item.dependencies.len() > MAX_DEPENDENCIES
            || item.dependencies.iter().copied().collect::<BTreeSet<_>>().len()
                != item.dependencies.len()
            || item.dependencies.iter().any(|id| {
                *id == item.selection.id || !earlier.iter().any(|prior| prior.selection.id == *id)
            })
            || item.correction_of.is_some_and(|selected| {
                !earlier.iter().any(|prior| {
                    prior.selection == selected && prior.state == InputState::Incorporated
                })
            })
            || (item.state == InputState::Incorporated) != item.invocation.is_some()
        {
            return Err(ControlError::InvalidInput);
        }
        if self.latest(item.selection.id).is_some_and(|latest| latest.selection == item.selection)
            && item.state == InputState::Superseded
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }

    fn validate_order(&self) -> Result<(), ControlError> {
        let pending: BTreeSet<_> = self
            .revisions
            .iter()
            .filter(|item| matches!(item.state, InputState::Queued | InputState::Held))
            .map(|item| item.selection.id)
            .collect();
        if self.order.len() != pending.len()
            || self.order.iter().copied().collect::<BTreeSet<_>>() != pending
        {
            return Err(ControlError::InvalidInput);
        }
        for (index, id) in self.order.iter().enumerate() {
            let item = self.latest(*id).ok_or(ControlError::InvalidInput)?;
            for dependency in &item.dependencies {
                if self
                    .latest(*dependency)
                    .is_none_or(|prior| prior.state != InputState::Incorporated)
                    && !self.order[..index].contains(dependency)
                {
                    return Err(ControlError::InvalidInput);
                }
            }
        }
        Ok(())
    }

    fn validate_bindings(&self) -> Result<(), ControlError> {
        let mut invocations = BTreeSet::new();
        let mut incorporated = BTreeSet::new();
        for binding in &self.invocations {
            if !invocations.insert(binding.invocation) || binding.items.len() > MAX_REVISIONS {
                return Err(ControlError::InvalidInput);
            }
            for selected in &binding.items {
                let item = self.exact(*selected)?;
                if item.state != InputState::Incorporated
                    || item.invocation != Some(binding.invocation)
                    || item.dependencies.iter().any(|id| !incorporated.contains(id))
                    || !incorporated.insert(selected.id)
                {
                    return Err(ControlError::InvalidInput);
                }
            }
        }
        if self.revisions.iter().filter(|item| item.state == InputState::Incorporated).count()
            != incorporated.len()
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(())
    }
}
