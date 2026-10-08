//! Read-only request candidates. Capturing never incorporates an input or changes its state.

use super::{ControlError, InputLedger, InputSelection, InputState};
use crate::control::MAX_REQUEST_CONTEXT_BYTES;
use std::collections::{BTreeMap, BTreeSet};

const SOURCE_NOTICE: &str = "One or more complete conversation messages are retained outside this fixed prompt. Read every user_request body marked requiresRead through request_sources and request_source_read before acting. assistant_history bodies are optional prior assistant output and carry no user authority.";
const USER_PREFIX: &str = "User: ";
const HOST_PREFIX: &str = "Host: ";
const REPLY_PREFIX: &str = "\n\nPeritus (public reply): ";

/// Exact input view used while preparing a request; it grants no execution authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputCapture {
    generation: u64,
    pending: Vec<InputSelection>,
    included: Vec<InputSelection>,
    out_of_line: Vec<InputSelection>,
    public_replies: Vec<super::InvocationId>,
    out_of_line_replies: Vec<super::InvocationId>,
    conversation: String,
}
impl InputCapture {
    pub(in crate::control) fn with_brief(
        mut self,
        brief: &crate::control::TaskBrief,
        inputs: &InputLedger,
    ) -> Result<Self, ControlError> {
        brief.append_to(&mut self.conversation, &self.included, inputs)?;
        Ok(self)
    }
    /// Returns the input generation, including hold, withdrawal and ordering transitions.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows new exact selections, in request order, excluding held/blocked inputs.
    #[must_use]
    pub fn pending(&self) -> &[InputSelection] {
        &self.pending
    }
    /// Borrows every incorporated or pending revision included in the governing view, in order.
    #[must_use]
    pub fn included(&self) -> &[InputSelection] {
        &self.included
    }
    /// Borrows exact governing revisions whose bodies remain outside the fixed prompt.
    #[must_use]
    pub fn out_of_line(&self) -> &[InputSelection] {
        &self.out_of_line
    }
    /// Borrows source invocation identities of public replies included in this view.
    #[must_use]
    pub fn public_replies(&self) -> &[super::InvocationId] {
        &self.public_replies
    }
    /// Borrows prior public replies whose exact bodies remain outside the fixed prompt.
    #[must_use]
    pub fn out_of_line_replies(&self) -> &[super::InvocationId] {
        &self.out_of_line_replies
    }
    /// Borrows incorporated history followed by eligible pending instructions.
    #[must_use]
    pub fn conversation(&self) -> &str {
        &self.conversation
    }
}

impl InputLedger {
    /// Captures a deterministic next-request input view without mutating the ledger.
    /// A queued input with a held prerequisite remains visible in the queue but is excluded
    /// from this view. Releasing that prerequisite changes the generation and invalidates it.
    ///
    /// # Errors
    /// Rejects invalid imported state or a request beyond the wire text resource bound.
    pub fn capture(&self) -> Result<InputCapture, ControlError> {
        self.capture_pending(true)
    }

    /// Renders only already incorporated inputs for immutable role-request scaffolding.
    /// Pending text is supplied separately at the guarded D0 boundary, so an acknowledged
    /// withdrawal cannot remain embedded in an earlier prepared role prompt or design.
    ///
    /// # Errors
    /// Rejects invalid durable history.
    pub fn incorporated_conversation(&self) -> Result<String, ControlError> {
        self.capture_pending(false).map(|capture| capture.conversation)
    }

    /// Captures public reply artifacts between their preceding invocation and later user input.
    /// The host verifies each supplied artifact's digest and scope before using this pure view.
    /// Trailing replies are retained but excluded until a later user instruction arrives.
    ///
    /// # Errors
    /// Rejects malformed ledger state or a reply without a matching admitted invocation.
    pub fn capture_with_replies(
        &self,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
    ) -> Result<InputCapture, ControlError> {
        self.capture_with_reply_view(
            replies,
            include_pending,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
        )
    }

    pub(in crate::control) fn inline_reply_sources(
        &self,
        reply_bytes: &BTreeMap<super::InvocationId, u64>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<Vec<super::InvocationId>, ControlError> {
        let captured = self.capture_pending(include_pending)?;
        validate_reply_sources(self, reply_bytes, &BTreeMap::new(), pinned, excluded)?;
        reply_plan(
            self,
            &captured,
            reply_bytes,
            pinned,
            excluded,
            replacements,
        )
        .map(|plan| {
            plan.into_iter()
                .filter_map(|(invocation, inline, replacement)| {
                    (inline && !replacement).then_some(invocation)
                })
                .collect()
        })
    }

    pub(in crate::control) fn reply_source_projection(
        &self,
        reply_bytes: &BTreeMap<super::InvocationId, u64>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<
        (
            Vec<super::InvocationId>,
            Vec<super::InvocationId>,
            Vec<InputSelection>,
        ),
        ControlError,
    > {
        let captured = self.capture_pending(include_pending)?;
        validate_reply_sources(self, reply_bytes, &BTreeMap::new(), pinned, excluded)?;
        let plan = reply_plan(
            self,
            &captured,
            reply_bytes,
            pinned,
            excluded,
            replacements,
        )?;
        let placeholders = plan
            .into_iter()
            .filter(|(_, inline, replacement)| *inline && !*replacement)
            .map(|(invocation, _, _)| {
                let bytes = usize::try_from(
                    *reply_bytes.get(&invocation).ok_or(ControlError::InvalidInput)?,
                )
                .map_err(|_| ControlError::Capacity)?;
                Ok((invocation, "x".repeat(bytes)))
            })
            .collect::<Result<BTreeMap<_, _>, ControlError>>()?;
        let projected = self.capture_with_reply_source_view(
            reply_bytes,
            &placeholders,
            include_pending,
            pinned,
            excluded,
            replacements,
        )?;
        Ok((
            projected.public_replies,
            projected.out_of_line_replies,
            projected.out_of_line,
        ))
    }

    pub(in crate::control) fn capture_with_reply_sources(
        &self,
        reply_bytes: &BTreeMap<super::InvocationId, u64>,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<InputCapture, ControlError> {
        self.capture_with_reply_source_view(
            reply_bytes,
            replies,
            include_pending,
            pinned,
            excluded,
            replacements,
        )
    }

    pub(in crate::control) fn capture_with_reply_view(
        &self,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<InputCapture, ControlError> {
        let reply_bytes = replies
            .iter()
            .map(|(invocation, reply)| {
                u64::try_from(reply.len())
                    .map(|bytes| (*invocation, bytes))
                    .map_err(|_| ControlError::Capacity)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        self.capture_with_reply_source_view(
            &reply_bytes,
            replies,
            include_pending,
            pinned,
            excluded,
            replacements,
        )
    }

    fn capture_with_reply_source_view(
        &self,
        reply_bytes: &BTreeMap<super::InvocationId, u64>,
        replies: &BTreeMap<super::InvocationId, String>,
        include_pending: bool,
        pinned: &BTreeSet<super::InvocationId>,
        excluded: &BTreeSet<super::InvocationId>,
        replacements: &BTreeMap<super::InvocationId, String>,
    ) -> Result<InputCapture, ControlError> {
        let mut captured = self.capture_pending(include_pending)?;
        validate_reply_sources(self, reply_bytes, replies, pinned, excluded)?;
        let plan = reply_plan(
            self,
            &captured,
            reply_bytes,
            pinned,
            excluded,
            replacements,
        )?;
        let inline_replies = plan
            .iter()
            .filter_map(|(invocation, inline, _)| inline.then_some(*invocation))
            .collect::<BTreeSet<_>>();
        let mut reserved_reply_bytes = plan.iter().try_fold(0_usize, |total, (invocation, inline, _)| {
            if !inline {
                return Ok(total);
            }
            rendered_reply_bytes(*invocation, reply_bytes, replacements)
                .and_then(|bytes| bytes.ok_or(ControlError::Capacity))
                .and_then(|bytes| total.checked_add(bytes).ok_or(ControlError::Capacity))
        })?;
        let mut conversation = String::new();
        let mut source_notice_added = false;
        let mut out_of_line = Vec::new();
        for binding in &self.invocations {
            for selected in &binding.items {
                append_input(
                    &mut conversation,
                    &mut source_notice_added,
                    &mut out_of_line,
                    reserved_reply_bytes,
                    self.latest(selected.id()).ok_or(ControlError::InvalidInput)?,
                )?;
            }
            if plan.iter().any(|(invocation, _, _)| *invocation == binding.invocation)
                && let Some(bytes) = reply_bytes.get(&binding.invocation)
            {
                captured.public_replies.push(binding.invocation);
                let replacement = replacements.get(&binding.invocation);
                if !inline_replies.contains(&binding.invocation) || replacement.is_some() {
                    captured.out_of_line_replies.push(binding.invocation);
                }
                if !inline_replies.contains(&binding.invocation) {
                    continue;
                }
                let rendered = rendered_reply_bytes(
                    binding.invocation,
                    reply_bytes,
                    replacements,
                )?
                .ok_or(ControlError::Capacity)?;
                let rendered_reply = match replacement {
                    Some(replacement) => replacement.as_str(),
                    None => {
                        let reply = replies
                            .get(&binding.invocation)
                            .ok_or(ControlError::InvalidInput)?;
                        if u64::try_from(reply.len()).ok() != Some(*bytes) {
                            return Err(ControlError::InvalidInput);
                        }
                        reply.as_str()
                    }
                };
                reserved_reply_bytes = reserved_reply_bytes
                    .checked_sub(rendered)
                    .ok_or(ControlError::InvalidInput)?;
                if conversation
                    .len()
                    .saturating_add(rendered)
                    .saturating_add(reserved_reply_bytes)
                    > MAX_REQUEST_CONTEXT_BYTES
                {
                    if !captured.out_of_line_replies.contains(&binding.invocation) {
                        captured.out_of_line_replies.push(binding.invocation);
                    }
                    continue;
                }
                conversation.push_str(REPLY_PREFIX);
                conversation.push_str(rendered_reply);
            }
        }
        for selected in &captured.pending {
            append_input(
                &mut conversation,
                &mut source_notice_added,
                &mut out_of_line,
                reserved_reply_bytes,
                self.latest(selected.id()).ok_or(ControlError::InvalidInput)?,
            )?;
        }
        captured.out_of_line = out_of_line;
        captured.conversation = conversation;
        Ok(captured)
    }

    fn capture_pending(&self, include_pending: bool) -> Result<InputCapture, ControlError> {
        self.validate()?;
        let mut available = BTreeSet::new();
        let mut conversation = String::new();
        let mut source_notice_added = false;
        let mut out_of_line = Vec::new();
        let mut included = Vec::new();
        for binding in &self.invocations {
            for selected in &binding.items {
                let item = self.latest(selected.id()).ok_or(ControlError::InvalidInput)?;
                append_input(
                    &mut conversation,
                    &mut source_notice_added,
                    &mut out_of_line,
                    0,
                    item,
                )?;
                included.push(*selected);
                available.insert(selected.id());
            }
        }
        let mut pending = Vec::new();
        for id in self.order.iter().filter(|_| include_pending) {
            let item = self.latest(*id).ok_or(ControlError::InvalidInput)?;
            if item.state() == InputState::Queued
                && item.dependencies().iter().all(|dependency| available.contains(dependency))
            {
                pending.push(item.selection());
                included.push(item.selection());
                append_input(
                    &mut conversation,
                    &mut source_notice_added,
                    &mut out_of_line,
                    0,
                    item,
                )?;
                available.insert(*id);
            }
        }
        Ok(InputCapture {
            generation: self.generation,
            pending,
            included,
            out_of_line,
            public_replies: Vec::new(),
            out_of_line_replies: Vec::new(),
            conversation,
        })
    }
}

fn validate_reply_sources(
    ledger: &InputLedger,
    reply_bytes: &BTreeMap<super::InvocationId, u64>,
    replies: &BTreeMap<super::InvocationId, String>,
    pinned: &BTreeSet<super::InvocationId>,
    excluded: &BTreeSet<super::InvocationId>,
) -> Result<(), ControlError> {
    if reply_bytes.iter().any(|(id, bytes)| {
        *bytes == 0 || !ledger.invocations.iter().any(|binding| binding.invocation == *id)
    }) || replies.iter().any(|(id, body)| {
        reply_bytes.get(id).copied() != u64::try_from(body.len()).ok()
    }) || pinned.iter().any(|id| !reply_bytes.contains_key(id))
        || pinned.iter().any(|id| excluded.contains(id))
    {
        return Err(ControlError::InvalidInput);
    }
    Ok(())
}

fn reply_plan(
    ledger: &InputLedger,
    captured: &InputCapture,
    reply_bytes: &BTreeMap<super::InvocationId, u64>,
    pinned: &BTreeSet<super::InvocationId>,
    excluded: &BTreeSet<super::InvocationId>,
    replacements: &BTreeMap<super::InvocationId, String>,
) -> Result<Vec<(super::InvocationId, bool, bool)>, ControlError> {
    let through = if captured.pending.is_empty() {
        ledger
            .invocations
            .iter()
            .rposition(|binding| !binding.items.is_empty())
            .unwrap_or(0)
    } else {
        ledger.invocations.len()
    };
    let eligible = ledger
        .invocations
        .iter()
        .enumerate()
        .filter(|(index, binding)| {
            (*index < through || pinned.contains(&binding.invocation))
                && !excluded.contains(&binding.invocation)
        })
        .filter_map(|(_, binding)| {
            reply_bytes
                .contains_key(&binding.invocation)
                .then_some(binding.invocation)
        })
        .collect::<Vec<_>>();
    let reply_budget = MAX_REQUEST_CONTEXT_BYTES.saturating_sub(source_notice_bytes());
    let mut inline = BTreeSet::new();
    let mut reserved = 0_usize;
    for invocation in eligible.iter().rev() {
        let Some(rendered) = rendered_reply_bytes(*invocation, reply_bytes, replacements)? else {
            continue;
        };
        if rendered <= reply_budget.saturating_sub(reserved) {
            inline.insert(*invocation);
            reserved = reserved.checked_add(rendered).ok_or(ControlError::Capacity)?;
        }
    }
    Ok(eligible
        .into_iter()
        .map(|invocation| {
            (
                invocation,
                inline.contains(&invocation),
                replacements.contains_key(&invocation),
            )
        })
        .collect())
}

fn rendered_reply_bytes(
    invocation: super::InvocationId,
    reply_bytes: &BTreeMap<super::InvocationId, u64>,
    replacements: &BTreeMap<super::InvocationId, String>,
) -> Result<Option<usize>, ControlError> {
    let body = match replacements.get(&invocation) {
        Some(replacement) => replacement.len(),
        None => {
            let bytes = *reply_bytes.get(&invocation).ok_or(ControlError::InvalidInput)?;
            if bytes > MAX_REQUEST_CONTEXT_BYTES as u64 {
                return Ok(None);
            }
            usize::try_from(bytes).map_err(|_| ControlError::Capacity)?
        }
    };
    body.checked_add(REPLY_PREFIX.len())
        .map(Some)
        .ok_or(ControlError::Capacity)
}

fn append_input(
    conversation: &mut String,
    source_notice_added: &mut bool,
    out_of_line: &mut Vec<InputSelection>,
    reserved_bytes: usize,
    input: &super::InputRevision,
) -> Result<(), ControlError> {
    let needs_source = input.source().is_some()
        || !fits_with_notice_reserve(
            conversation,
            input.text(),
            *source_notice_added,
            reserved_bytes,
        );
    if needs_source {
        out_of_line.push(input.selection());
        ensure_source_notice(conversation, source_notice_added, reserved_bytes)?;
        Ok(())
    } else {
        append(conversation, input.text())
    }
}

fn fits_with_notice_reserve(
    conversation: &str,
    text: &str,
    notice_added: bool,
    reserved_bytes: usize,
) -> bool {
    let append_bytes = text
        .len()
        .saturating_add(6)
        .saturating_add(if conversation.is_empty() { 0 } else { 2 });
    let reserve = if notice_added {
        0
    } else {
        source_notice_bytes()
    };
    conversation
        .len()
        .saturating_add(append_bytes)
        .saturating_add(reserve)
        .saturating_add(reserved_bytes)
        <= MAX_REQUEST_CONTEXT_BYTES
}

fn append(conversation: &mut String, text: &str) -> Result<(), ControlError> {
    let separator_bytes = if conversation.is_empty() { 0 } else { 2 };
    if conversation
        .len()
        .saturating_add(text.len())
        .saturating_add(separator_bytes + USER_PREFIX.len())
        > MAX_REQUEST_CONTEXT_BYTES
    {
        return Err(ControlError::Capacity);
    }
    if !conversation.is_empty() {
        conversation.push_str("\n\n");
    }
    conversation.push_str(USER_PREFIX);
    conversation.push_str(text);
    Ok(())
}

fn source_notice_bytes() -> usize {
    SOURCE_NOTICE
        .len()
        .saturating_add(HOST_PREFIX.len())
        .saturating_add(2)
}

fn ensure_source_notice(
    conversation: &mut String,
    source_notice_added: &mut bool,
    reserved_bytes: usize,
) -> Result<(), ControlError> {
    if *source_notice_added {
        return Ok(());
    }
    let separator_bytes = if conversation.is_empty() { 0 } else { 2 };
    if conversation
        .len()
        .saturating_add(SOURCE_NOTICE.len())
        .saturating_add(HOST_PREFIX.len())
        .saturating_add(separator_bytes)
        .saturating_add(reserved_bytes)
        > MAX_REQUEST_CONTEXT_BYTES
    {
        return Err(ControlError::Capacity);
    }
    if !conversation.is_empty() {
        conversation.push_str("\n\n");
    }
    conversation.push_str(HOST_PREFIX);
    conversation.push_str(SOURCE_NOTICE);
    *source_notice_added = true;
    Ok(())
}
