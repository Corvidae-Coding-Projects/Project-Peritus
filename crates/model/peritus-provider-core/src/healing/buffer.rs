//! Preserve valid provider fragments verbatim while delaying admission until argument close.

use super::{
    CanonicalJson, ItemId, JsonBounds, ModelEvent, ProtocolLimits, ProviderCoreError,
    StreamFragment, ToolCallId, invalid,
};

/// Bounded raw arguments and their original fragment boundaries.
#[derive(Default)]
pub struct ToolArgumentBuffer {
    bytes: Vec<u8>,
    ends: Vec<usize>,
}

impl ToolArgumentBuffer {
    /// Creates an empty bounded buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: Vec::new(), ends: Vec::new() }
    }

    /// Borrows raw provider bytes for exact terminal consistency checks.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Appends one bounded provider observation without exposing it as an admitted tool argument.
    ///
    /// # Errors
    /// Rejects byte overflow under the selected tool-argument policy.
    pub fn append(
        &mut self,
        bytes: &[u8],
        limits: ProtocolLimits,
    ) -> Result<(), ProviderCoreError> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > limits.max_tool_argument_bytes())
        {
            return Err(invalid());
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| invalid())?;
        self.ends.try_reserve(1).map_err(|_| invalid())?;
        self.bytes.extend_from_slice(bytes);
        self.ends.push(self.bytes.len());
        Ok(())
    }

    /// Emits original fragments for valid JSON, or audited repaired fragments for invalid JSON.
    ///
    /// # Errors
    /// Rejects invalid objects and repairs outside the syntax-only policy.
    pub fn complete(
        &self,
        call_id: &ToolCallId,
        limits: ProtocolLimits,
    ) -> Result<Vec<ModelEvent>, ProviderCoreError> {
        let text = std::str::from_utf8(&self.bytes).map_err(|_| invalid())?;
        if CanonicalJson::parse(text, JsonBounds::value(limits))
            .is_ok_and(|value| value.is_object())
        {
            let mut start = 0;
            let mut events = Vec::new();
            events.try_reserve_exact(self.ends.len()).map_err(|_| invalid())?;
            for end in &self.ends {
                for chunk in self.bytes[start..*end].chunks(limits.max_event_bytes()) {
                    let fragment = StreamFragment::new(owned(chunk)?, limits)
                        .map_err(|_| invalid())?;
                    events.push(ModelEvent::ToolArgumentDelta {
                        call_id: call_id.clone(),
                        fragment,
                    });
                }
                start = *end;
            }
            return Ok(events);
        }
        super::tool_arguments(&self.bytes, call_id, limits)
    }

    /// Validates the complete arguments and returns a resumable final-output cursor.
    ///
    /// Progress observations remain unauthoritative; only events returned by this cursor are
    /// eligible for completed tool-call reduction.
    pub fn into_completion(
        self,
        call_id: ToolCallId,
        limits: ProtocolLimits,
    ) -> Result<JsonCompletionCursor, ProviderCoreError> {
        let Self { bytes, ends } = self;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        if CanonicalJson::parse(text, JsonBounds::value(limits))
            .is_ok_and(|value| value.is_object())
        {
            return JsonCompletionCursor::new(
                CompletionTarget::Tool(call_id),
                bytes,
                ends,
                None,
                limits,
            );
        }
        let (value, audit) = super::object(text, call_id.expose_for_wire(), limits)?.into_parts();
        JsonCompletionCursor::canonical(
            CompletionTarget::Tool(call_id),
            value.canonical_bytes(),
            audit,
            limits,
        )
    }
}

/// Bounded raw structured output and its original provider-fragment boundaries.
#[derive(Default)]
pub struct StructuredOutputBuffer {
    bytes: Vec<u8>,
    ends: Vec<usize>,
}

impl StructuredOutputBuffer {
    /// Creates an empty structured-output buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self { bytes: Vec::new(), ends: Vec::new() }
    }

    /// Appends one provider observation under the selected output and event policies.
    pub fn append(
        &mut self,
        bytes: &[u8],
        limits: ProtocolLimits,
    ) -> Result<(), ProviderCoreError> {
        if self.ends.len() >= limits.max_events()
            || self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_none_or(|length| length > limits.max_output_bytes())
        {
            return Err(invalid());
        }
        self.bytes.try_reserve(bytes.len()).map_err(|_| invalid())?;
        self.ends.try_reserve(1).map_err(|_| invalid())?;
        self.bytes.extend_from_slice(bytes);
        self.ends.push(self.bytes.len());
        Ok(())
    }

    /// Validates or syntax-heals the closed value and returns a resumable final-output cursor.
    pub fn into_completion(
        self,
        item_id: ItemId,
        limits: ProtocolLimits,
    ) -> Result<JsonCompletionCursor, ProviderCoreError> {
        let Self { bytes, ends } = self;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        if CanonicalJson::parse(text, JsonBounds::value(limits)).is_ok() {
            return JsonCompletionCursor::new(
                CompletionTarget::Structured(item_id),
                bytes,
                ends,
                None,
                limits,
            );
        }
        let (value, audit) = super::parse(
            text,
            item_id.expose_for_wire(),
            limits,
            super::Context::StructuredOutput,
        )?
        .into_parts();
        JsonCompletionCursor::canonical(
            CompletionTarget::Structured(item_id),
            value.canonical_bytes(),
            audit,
            limits,
        )
    }
}

enum CompletionTarget {
    Tool(ToolCallId),
    Structured(ItemId),
}

/// Pull-based validated JSON completion that allocates at most one normalized fragment at a time.
pub struct JsonCompletionCursor {
    target: CompletionTarget,
    bytes: Vec<u8>,
    ends: Vec<usize>,
    boundary: usize,
    offset: usize,
    audit: Option<ModelEvent>,
    remaining: usize,
    limits: ProtocolLimits,
}

impl JsonCompletionCursor {
    fn canonical(
        target: CompletionTarget,
        bytes: &[u8],
        audit: Option<ModelEvent>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProviderCoreError> {
        let bytes = owned(bytes)?;
        let mut ends = Vec::new();
        ends.try_reserve_exact(1).map_err(|_| invalid())?;
        ends.push(bytes.len());
        Self::new(target, bytes, ends, audit, limits)
    }

    fn new(
        target: CompletionTarget,
        bytes: Vec<u8>,
        ends: Vec<usize>,
        audit: Option<ModelEvent>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProviderCoreError> {
        let mut start = 0_usize;
        let mut fragments = 0_usize;
        for end in &ends {
            if *end < start || *end > bytes.len() {
                return Err(invalid());
            }
            let length = *end - start;
            if length > 0 {
                let adjustment = limits.max_event_bytes().checked_sub(1).ok_or_else(invalid)?;
                let count = length
                    .checked_add(adjustment)
                    .map(|value| value / limits.max_event_bytes())
                    .ok_or_else(invalid)?;
                fragments = fragments.checked_add(count).ok_or_else(invalid)?;
            }
            start = *end;
        }
        if start != bytes.len() || fragments == 0 {
            return Err(invalid());
        }
        let remaining = fragments
            .checked_add(usize::from(audit.is_some()))
            .ok_or_else(invalid)?;
        Ok(Self {
            target,
            bytes,
            ends,
            boundary: 0,
            offset: 0,
            audit,
            remaining,
            limits,
        })
    }

    /// Returns the exact number of events that remain to be pulled.
    #[must_use]
    pub const fn remaining_events(&self) -> usize {
        self.remaining
    }

    /// Pulls one audit or validated final-output event.
    pub fn next_event(&mut self) -> Result<Option<ModelEvent>, ProviderCoreError> {
        if let Some(audit) = self.audit.take() {
            self.remaining = self.remaining.checked_sub(1).ok_or_else(invalid)?;
            return Ok(Some(audit));
        }
        while let Some(end) = self.ends.get(self.boundary).copied() {
            if self.offset == end {
                self.boundary = self.boundary.checked_add(1).ok_or_else(invalid)?;
                continue;
            }
            let next = self
                .offset
                .saturating_add(self.limits.max_event_bytes())
                .min(end);
            let fragment = StreamFragment::new(owned(&self.bytes[self.offset..next])?, self.limits)
                .map_err(|_| invalid())?;
            self.offset = next;
            self.remaining = self.remaining.checked_sub(1).ok_or_else(invalid)?;
            return Ok(Some(match &self.target {
                CompletionTarget::Tool(call_id) => {
                    ModelEvent::ToolArgumentDelta { call_id: call_id.clone(), fragment }
                }
                CompletionTarget::Structured(item_id) => {
                    ModelEvent::TextDelta { item_id: item_id.clone(), fragment }
                }
            }));
        }
        if self.remaining != 0 {
            return Err(invalid());
        }
        Ok(None)
    }
}

fn owned(bytes: &[u8]) -> Result<Vec<u8>, ProviderCoreError> {
    let mut owned = Vec::new();
    owned.try_reserve_exact(bytes.len()).map_err(|_| invalid())?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}
