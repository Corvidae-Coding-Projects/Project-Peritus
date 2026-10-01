//! Selects the one authoritative host-visible result from a completed runtime transcript.

use std::collections::BTreeSet;

use peritus_model_protocol::{ModelEvent, ProtocolLimits};

use super::{DecodeFailure, RuntimeTurn, State, StructuredTurn, validate_turn};

pub(super) fn select_turn(
    state: &State,
    final_message: Option<&str>,
    allowed_tools: &BTreeSet<String>,
    call_bounds: std::ops::RangeInclusive<usize>,
) -> Result<RuntimeTurn, DecodeFailure> {
    let messages = &state.assistant_messages;
    let last = messages.last().ok_or(DecodeFailure::InvalidEnvelope)?;
    let Some(expected) = final_message else {
        if messages.len() != 1 {
            return Err(DecodeFailure::MultipleMessages);
        }
        let (turn, repairs) = decode_structured(last)?;
        return validate_turn(turn, allowed_tools, call_bounds, state, repairs);
    };
    if last.trim() != expected.trim() {
        return Err(DecodeFailure::InvalidLifecycle);
    }
    for encoded in messages {
        let Ok((turn, repairs)) = decode_structured(encoded) else {
            continue;
        };
        if !turn.tool_calls.is_empty() {
            return validate_turn(turn, allowed_tools, call_bounds, state, repairs);
        }
    }
    let (turn, repairs) = decode_structured(last)?;
    validate_turn(turn, allowed_tools, call_bounds, state, repairs)
}

fn decode_structured(encoded: &str) -> Result<(StructuredTurn, Vec<ModelEvent>), DecodeFailure> {
    let (value, audit) =
        peritus_provider_core::healing::object(encoded, "codex.turn", ProtocolLimits::PRODUCTION)
            .map_err(|_| DecodeFailure::InvalidEnvelope)?
            .into_parts();
    let turn: StructuredTurn = serde_json::from_slice(value.canonical_bytes())
        .map_err(|_| DecodeFailure::InvalidEnvelope)?;
    Ok((turn, audit.into_iter().collect()))
}
