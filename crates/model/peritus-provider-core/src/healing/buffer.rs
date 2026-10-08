//! Preserve valid provider fragments verbatim while delaying admission until argument close.

use super::{
    CanonicalJson, JsonBounds, ModelEvent, ProtocolLimits, ProviderCoreError, StreamFragment,
    ToolCallId, invalid,
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

    /// Appends a nonempty bounded fragment without exposing it as an admitted tool argument.
    ///
    /// # Errors
    /// Rejects empty fragments, byte overflow, and excessive fragmentation.
    pub fn append(
        &mut self,
        bytes: &[u8],
        limits: ProtocolLimits,
    ) -> Result<(), ProviderCoreError> {
        if bytes.is_empty()
            || bytes.len() > limits.max_event_bytes()
            || self.ends.len() >= 65_536
            || self
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
                let mut bytes = Vec::new();
                bytes.try_reserve_exact(*end - start).map_err(|_| invalid())?;
                bytes.extend_from_slice(&self.bytes[start..*end]);
                let fragment = StreamFragment::new(bytes, limits)
                    .map_err(|_| invalid())?;
                events.push(ModelEvent::ToolArgumentDelta { call_id: call_id.clone(), fragment });
                start = *end;
            }
            return Ok(events);
        }
        super::tool_arguments(&self.bytes, call_id, limits)
    }
}
