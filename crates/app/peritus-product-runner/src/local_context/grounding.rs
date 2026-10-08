//! Scoped reconstruction of host-validated repository grounding evidence.

use std::collections::BTreeMap;

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, ContentBlock, JsonBounds, ProtocolLimits, decode_messages,
};

use super::{
    error,
    memory::LocalMemory,
    record::ArchiveKind,
};
use crate::developer_tools::GroundingEvidence;

impl LocalMemory {
    pub(super) fn recover_grounding(
        &self,
        expected_prefix: &str,
    ) -> Result<GroundingEvidence, DeveloperLoopError> {
        let mut grounding = GroundingEvidence::for_workspace(&self.workspace);
        self.replay_tool_observations(expected_prefix, None, &mut |call, output| {
            grounding.record_completed(call, output);
        })?;
        Ok(grounding)
    }

    pub(super) fn recover_grounding_scope(
        &self,
        expected_scope: &str,
        expected_revision: u64,
    ) -> Result<GroundingEvidence, DeveloperLoopError> {
        let mut grounding = GroundingEvidence::for_workspace(&self.workspace);
        for prefix in self.grounding_prefixes_for_scope(expected_scope, expected_revision)? {
            self.replay_tool_observations(&prefix, None, &mut |call, output| {
                grounding.record_completed(call, output);
            })?;
        }
        Ok(grounding)
    }

    pub(super) fn replay_tool_observations(
        &self,
        expected_prefix: &str,
        tool_name: Option<&str>,
        observe: &mut dyn FnMut(&CompletedToolCall, &CanonicalJson),
    ) -> Result<(), DeveloperLoopError> {
        let sources = self.grounding_observations_for_prefix(expected_prefix)?;

        let mut calls = BTreeMap::<(u64, String, [u8; 32]), CompletedToolCall>::new();
        for source in sources
            .iter()
            .filter(|source| source.kind == ArchiveKind::Assistant)
        {
            for message in decode_messages(
                &self.artifact(source.sequence)?,
                ProtocolLimits::PRODUCTION,
            )? {
                for block in message.content() {
                    if let ContentBlock::ToolCall(call) = block {
                        if tool_name.is_some_and(|name| name != call.name().as_str()) {
                            continue;
                        }
                        calls.insert(
                            (
                                source.invocation,
                                call.id().expose_for_wire().to_owned(),
                                peritus_codec::sha256(call.arguments().canonical_bytes())
                                    .into_bytes(),
                            ),
                            call.clone(),
                        );
                    }
                }
            }
        }

        for source in sources
            .iter()
            .filter(|source| source.kind == ArchiveKind::ToolOutput && !source.is_error)
        {
            let identity = source
                .call
                .as_ref()
                .ok_or_else(|| error("grounding observation has no call identity"))?;
            if tool_name.is_some_and(|name| name != identity.name) {
                continue;
            }
            let Some(call) = calls.get(&(
                source.invocation,
                identity.id.clone(),
                identity.arguments_digest,
            )) else {
                continue;
            };
            if call.name().as_str() != identity.name {
                return Err(error("grounding call identity mismatch"));
            }
            let bytes = self.artifact(source.sequence)?;
            let encoded = std::str::from_utf8(&bytes)
                .map_err(|_| error("grounding observation is not UTF-8 JSON"))?;
            let output = CanonicalJson::parse(
                encoded,
                JsonBounds::value(ProtocolLimits::PRODUCTION),
            )?;
            observe(call, &output);
        }
        Ok(())
    }
}

pub(super) fn valid_invocation(request_prefix: &str, logical_prefix: &str) -> bool {
    let Some(suffix) = request_prefix.strip_prefix(logical_prefix) else { return false };
    let Some((sequence, nonce)) = suffix.split_once('-') else { return false };
    !sequence.is_empty()
        && (sequence == "0" || !sequence.starts_with('0'))
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.parse::<u64>().is_ok()
        && nonce.len() == 32
        && nonce.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
