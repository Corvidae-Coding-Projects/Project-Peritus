//! Durable progress identities and truthful selected-evidence loop guidance.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::collections::VecDeque;

use peritus_model_protocol::{CanonicalJson, ContentBlock, JsonBounds, Message, ProtocolLimits};
use peritus_types::Sha256Digest;
use serde_json::Value;

#[cfg(test)]
const RECENT_OBSERVATIONS: usize = 16;
#[cfg(test)]
const WARN_REPEATS: u8 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::developer_tools) struct ProgressDelta {
    new_evidence: bool,
    discharged_obligation: bool,
    candidate_advanced: bool,
}

impl ProgressDelta {
    pub(in crate::developer_tools) fn from_observation(
        name: &str,
        arguments: &Value,
        result: &Value,
        accepted: bool,
        discharged_obligation: bool,
        mutation_boundary: bool,
    ) -> Self {
        let purpose = arguments
            .get("purpose")
            .and_then(Value::as_str)
            .or_else(|| result.get("purpose").and_then(Value::as_str));
        let command_success = accepted
            && is_command(name)
            && result.get("success").and_then(Value::as_bool) == Some(true);
        Self {
            new_evidence: accepted && is_evidence(name),
            discharged_obligation: accepted
                && (discharged_obligation
                    || name == "workspace_scope"
                    || (command_success && purpose == Some("verification"))),
            candidate_advanced: accepted
                && ((mutation_boundary
                    && matches!(
                        name,
                        "workspace_write" | "workspace_patch" | "workspace_remove"
                    )
                    && result.get("changed").and_then(Value::as_bool) != Some(false))
                    || (command_success && purpose == Some("external_effect"))),
        }
    }

    pub(in crate::developer_tools) const fn candidate_advanced(self) -> bool {
        self.candidate_advanced
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProgressBinding {
    revision: u64,
    request_sources: [u8; 32],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ProgressRecord {
    new_evidence: bool,
    discharged_obligation: bool,
    candidate_advanced: bool,
}

impl ProgressRecord {
    fn merge(&mut self, delta: ProgressDelta) {
        self.new_evidence |= delta.new_evidence;
        self.discharged_obligation |= delta.discharged_obligation;
        self.candidate_advanced |= delta.candidate_advanced;
    }
}

#[derive(Clone, Default)]
pub(in crate::developer_tools) struct InspectionLedger {
    binding: Option<ProgressBinding>,
    observations: BTreeMap<Sha256Digest, ProgressRecord>,
}

impl InspectionLedger {
    pub(in crate::developer_tools) fn bind(
        &mut self,
        revision: u64,
        request_sources: [u8; 32],
    ) {
        let binding = ProgressBinding { revision, request_sources };
        if self.binding.is_some_and(|current| current != binding) {
            self.observations.clear();
        }
        self.binding = Some(binding);
    }

    pub(in crate::developer_tools) fn binding(&self) -> Option<(u64, [u8; 32])> {
        self.binding.map(|binding| (binding.revision, binding.request_sources))
    }

    pub(in crate::developer_tools) fn merge(&mut self, other: &Self) {
        match (self.binding, other.binding) {
            (Some(left), Some(right)) if left != right => return,
            (None, Some(binding)) => self.binding = Some(binding),
            _ => {}
        }
        for (identity, record) in &other.observations {
            let retained = self.observations.entry(*identity).or_default();
            retained.new_evidence |= record.new_evidence;
            retained.discharged_obligation |= record.discharged_obligation;
            retained.candidate_advanced |= record.candidate_advanced;
        }
    }

    pub(in crate::developer_tools) fn observe(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        delta: ProgressDelta,
    ) -> LedgerObservation {
        let identity = observation_identity(name, arguments, result);
        let retained = self.observations.entry(identity).or_default();
        let newly_observed_evidence = delta.new_evidence && !retained.new_evidence;
        retained.merge(delta);
        LedgerObservation {
            identity,
            progressed: newly_observed_evidence
                || delta.discharged_obligation
                || delta.candidate_advanced,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::developer_tools) struct LedgerObservation {
    identity: Sha256Digest,
    progressed: bool,
}

struct RepeatedObservation {
    tool: String,
    obligation: Option<String>,
}

#[derive(Default)]
pub(super) struct InspectionProgress {
    visible_observations: BTreeSet<Sha256Digest>,
    visibility_observed: bool,
    warning_pending: Option<RepeatedObservation>,
    #[cfg(test)]
    recent: VecDeque<Sha256Digest>,
    #[cfg(test)]
    repeats: u8,
}

impl InspectionProgress {
    pub(super) fn observe_model_context(&mut self, messages: &[Message]) {
        let mut pending = BTreeMap::new();
        let mut visible = BTreeSet::new();
        for block in messages.iter().flat_map(Message::content) {
            match block {
                ContentBlock::ToolCall(call) => {
                    if let Ok(arguments) =
                        serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
                    {
                        pending.insert(
                            call.id().expose_for_wire(),
                            (call.name().as_str(), arguments),
                        );
                    }
                }
                ContentBlock::ToolResult(result) => {
                    if let Some((name, arguments)) =
                        pending.remove(result.call_id().expose_for_wire())
                        && let Ok(output) =
                            serde_json::from_slice::<Value>(result.output().canonical_bytes())
                    {
                        visible.insert(observation_identity(name, &arguments, &output));
                    }
                }
                _ => {}
            }
        }
        #[cfg(test)]
        for digest in &visible {
            if self.recent.contains(digest) {
                continue;
            }
            if self.recent.len() == RECENT_OBSERVATIONS {
                self.recent.pop_front();
            }
            self.recent.push_back(*digest);
        }
        self.visible_observations = visible;
        self.visibility_observed = true;
    }

    pub(super) fn observe_progress(
        &mut self,
        name: &str,
        observation: LedgerObservation,
        stalled_obligation: Option<&str>,
    ) {
        if let Some(obligation) = stalled_obligation {
            self.warning_pending = Some(RepeatedObservation {
                tool: name.to_owned(),
                obligation: Some(obligation.to_owned()),
            });
            return;
        }
        if observation.progressed {
            self.warning_pending = None;
            return;
        }
        let visible = !self.visibility_observed
            || self.visible_observations.contains(&observation.identity);
        if visible {
            self.warning_pending = Some(RepeatedObservation {
                tool: name.to_owned(),
                obligation: None,
            });
        } else {
            // Re-reading exact bytes is legitimate when the selected model view evicted them.
            self.warning_pending = None;
        }
    }

    pub(super) fn feedback_for(
        &mut self,
        required: Option<&str>,
        read_only: bool,
    ) -> Option<String> {
        let repeated = self.warning_pending.take()?;
        let obligation = required.map(str::to_owned).or(repeated.obligation);
        if let Some(obligation) = obligation {
            if obligation != repeated.tool {
                return Some(format!(
                    "The progress ledger still has the unchanged `{obligation}` obligation after `{}`. Call `{obligation}` now. If its visible result reports unavailable authority, revoked permission, missing input, or unresolved effect recovery, return that exact recoverable blocker.",
                    repeated.tool,
                ));
            }
            return Some(format!(
                "The progress ledger already contains this exact visible `{}` observation, but the `{obligation}` obligation is unchanged. Use the tool's next cursor or a different task-relevant source; if the visible result shows that no such source is available, return that recoverable blocker.",
                repeated.tool,
            ));
        }
        let next = match repeated.tool.as_str() {
            "workspace_list" => {
                "read a relevant returned file with `workspace_read`, or use the listing to finish the current analysis"
            }
            "workspace_search" => {
                "follow the returned next cursor when present; otherwise read a returned location with workspace_read or use the completed search evidence"
            }
            "workspace_read" if read_only => {
                "follow the returned next cursor when present; otherwise use the completed visible file evidence or inspect a specifically named different source required by the task"
            }
            "workspace_read" => {
                "follow the returned next cursor when present; otherwise use the completed visible file evidence for the requested patch, write, or verification"
            }
            "request_sources" | "context_sources" => {
                "follow the returned next cursor, or use the completed source catalog"
            }
            "request_source_read" | "context_source_read" => {
                "follow the returned slice cursor, or use the completed source body"
            }
            "command_poll" | "command_recover" => {
                "use the visible terminal state; if the owned process cannot advance, return that process blocker"
            }
            _ if read_only => {
                "use the visible evidence to finish the current analysis, or inspect a specifically named missing source"
            }
            _ => {
                "use the visible observation for the requested candidate step or verification"
            }
        };
        Some(format!(
            "The progress ledger already contains this exact visible `{}` call and result, so it adds no new evidence or completed obligation. Next action: {next}. If authority, permission, missing input, or effect recovery prevents that action, return the visible recoverable blocker.",
            repeated.tool,
        ))
    }

    #[cfg(test)]
    pub(super) fn observe(
        &mut self,
        name: &str,
        arguments: &Value,
        result: &Value,
        mutation_boundary: bool,
    ) {
        if !is_inspection(name) {
            if mutation_boundary {
                self.recent.clear();
                self.repeats = 0;
                self.warning_pending = None;
            }
            return;
        }
        let digest = observation_identity(name, arguments, result);
        if self.recent.contains(&digest) {
            let visible = !self.visibility_observed
                || self.visible_observations.contains(&digest);
            if visible {
                self.repeats = self.repeats.saturating_add(1);
                if self.repeats == WARN_REPEATS {
                    self.warning_pending = Some(RepeatedObservation {
                        tool: name.to_owned(),
                        obligation: None,
                    });
                }
            } else {
                self.repeats = 0;
                self.warning_pending = None;
            }
        } else {
            self.repeats = 0;
            self.warning_pending = None;
            if self.recent.len() == RECENT_OBSERVATIONS {
                self.recent.pop_front();
            }
            self.recent.push_back(digest);
        }
    }

    #[cfg(test)]
    pub(super) fn feedback(&mut self) -> Option<String> {
        self.feedback_for(None, false)
    }
}

const fn is_inspection(name: &str) -> bool {
    matches!(name, "workspace_list" | "workspace_read" | "workspace_search")
}

fn is_evidence(name: &str) -> bool {
    matches!(
        name,
        "request_sources"
            | "request_source_read"
            | "context_sources"
            | "context_source_read"
            | "workspace_list"
            | "workspace_search"
            | "workspace_read"
            | "workspace_scope_evidence_read"
    ) || is_command(name)
}

fn is_command(name: &str) -> bool {
    name == "run_command" || name.starts_with("command_")
}

fn observation_identity(name: &str, arguments: &Value, result: &Value) -> Sha256Digest {
    let arguments = canonical_digest(arguments);
    let result = local_source_digest(result).unwrap_or_else(|| canonical_digest(result));
    let mut material = Vec::with_capacity(56 + name.len() + (Sha256Digest::LENGTH * 2));
    material.extend_from_slice(b"peritus/progress-observation/v1\0");
    material.extend_from_slice(&u64::try_from(name.len()).unwrap_or(u64::MAX).to_le_bytes());
    material.extend_from_slice(name.as_bytes());
    material.extend_from_slice(arguments.as_bytes());
    material.extend_from_slice(result.as_bytes());
    peritus_codec::sha256(&material)
}

fn canonical_digest(value: &Value) -> Sha256Digest {
    let encoded = value.to_string();
    CanonicalJson::parse(&encoded, JsonBounds::value(ProtocolLimits::PRODUCTION))
        .map_or_else(|_| peritus_codec::sha256(encoded.as_bytes()), |value| value.digest())
}

fn local_source_digest(result: &Value) -> Option<Sha256Digest> {
    let metadata = result.get("local_context")?.as_object()?;
    if metadata.get("authority")?.as_str()? != "none"
        || metadata.get("handle")?.as_str()?.is_empty()
        || metadata.get("base_revision")?.as_u64().is_none()
        || metadata.get("bytes")?.as_u64().is_none()
    {
        return None;
    }
    decode_digest(metadata.get("sha256")?.as_str()?)
}

fn decode_digest(encoded: &str) -> Option<Sha256Digest> {
    if encoded.len() != Sha256Digest::LENGTH * 2 {
        return None;
    }
    let mut bytes = [0_u8; Sha256Digest::LENGTH];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = hex_nibble(pair[0])?.checked_mul(16)?.checked_add(hex_nibble(pair[1])?)?;
    }
    Some(Sha256Digest::new(bytes))
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: impl Into<Value>) -> Value {
        Value::from_iter([("path", value.into())])
    }

    #[test]
    fn identical_and_alternating_inspections_warn_once() {
        for width in [1, 2] {
            let mut progress = InspectionProgress::default();
            for index in 0..width {
                progress.observe("workspace_read", &path(index), &Value::from("same"), false);
            }
            for index in 1..=32 {
                progress.observe(
                    "workspace_read",
                    &path(index % width),
                    &Value::from("same"),
                    false,
                );
                assert_eq!(progress.feedback().is_some(), index == WARN_REPEATS);
            }
            assert!(progress.feedback().is_none());
        }
    }

    #[test]
    fn new_evidence_and_noninspection_work_reset_the_cycle() {
        let mut progress = InspectionProgress::default();
        for index in 0..32 {
            progress.observe("workspace_read", &path("state"), &Value::from(index), false);
        }
        assert_eq!(progress.recent.len(), RECENT_OBSERVATIONS);
        for name in ["run_command", "command_poll", "workspace_write"] {
            for _ in 0..3 {
                progress.observe("workspace_list", &path("."), &Value::Array(Vec::new()), false);
                progress.feedback();
            }
            assert_eq!(progress.repeats, 2);
            progress.observe(
                name,
                &Value::Object(serde_json::Map::new()),
                &Value::from_iter([("success", Value::from(false))]),
                true,
            );
            assert_eq!(progress.repeats, 0);
            assert!(progress.feedback().is_none());
        }
    }

    #[test]
    fn a_repeated_batch_delivers_its_warning_once() {
        let mut progress = InspectionProgress::default();
        progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        for _ in 0..3 {
            progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        }
        assert!(progress.feedback().is_some());
        progress.observe("workspace_read", &path("state"), &Value::from("same"), false);
        assert!(progress.feedback().is_none());
    }
}
