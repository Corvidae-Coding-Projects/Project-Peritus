//! Durable receipts around mutating developer-tool effects.

mod codec;
mod inspection;
mod owner;
mod replay;
mod storage;

pub use inspection::{
    UncertainEffect, UncertainEffectState, acknowledge_uncertain_effect, uncertain_effects,
};

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::CompletedToolCall;
use peritus_types::{ActionId, ProcessId, RunId};
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::path::tool;

const FORMAT_VERSION: u32 = 2;

pub(super) enum ReceiptDecision {
    Execute,
    Replay { value: Value, is_error: bool },
    RecoverCheckpoint { value: Value, is_error: bool },
    RecoverCommandOwner { owner: NativeCommandOwner, scope: String, ordinal: u32 },
    Refuse { detail: String, ambiguous: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct NativeCommandOwner {
    pub(super) source_run: RunId,
    pub(super) execution_run: RunId,
    pub(super) action: ActionId,
    pub(super) process: ProcessId,
}

pub(super) struct EffectReceiptLedger {
    path: PathBuf,
    scope: String,
    next_ordinal: u32,
    loaded: bool,
    entries: BTreeMap<u32, ReceiptRecord>,
    all_entries: BTreeMap<(String, u32), ReceiptRecord>,
}

#[derive(Clone)]
struct ReceiptRecord {
    version: u32,
    scope: String,
    ordinal: u32,
    call_id: String,
    tool: String,
    request_sha256: String,
    state: ReceiptState,
    native_owner: Option<NativeCommandOwner>,
    owner_inactive: bool,
    output: Option<Value>,
    is_error: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReceiptState {
    Started,
    Applied,
    Completed,
    Ambiguous,
    Reviewed,
}

impl EffectReceiptLedger {
    pub(super) const fn new(path: PathBuf, scope: String) -> Self {
        Self {
            path,
            scope,
            next_ordinal: 0,
            loaded: false,
            entries: BTreeMap::new(),
            all_entries: BTreeMap::new(),
        }
    }

    pub(super) fn begin(
        &mut self,
        call: &CompletedToolCall,
    ) -> Result<ReceiptDecision, DeveloperLoopError> {
        self.load()?;
        self.next_ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| tool("effect receipt ordinal overflowed"))?;
        let ordinal = self.next_ordinal;
        let digest = request_digest(call);
        if let Some(existing) = self.entries.get(&ordinal).cloned() {
            return self.decision_for_existing(call, ordinal, &digest, existing);
        }
        if self.entries.values().any(|record| record.call_id == call.id().expose_for_wire()) {
            return Ok(ReceiptDecision::Refuse {
                detail: "provider reused one tool-call ID for more than one effect request"
                    .to_owned(),
                ambiguous: false,
            });
        }
        let record = ReceiptRecord {
            version: FORMAT_VERSION,
            scope: self.scope.clone(),
            ordinal,
            call_id: call.id().expose_for_wire().to_owned(),
            tool: call.name().as_str().to_owned(),
            request_sha256: digest,
            state: ReceiptState::Started,
            native_owner: None,
            owner_inactive: false,
            output: None,
            is_error: None,
        };
        self.append(&record)?;
        self.entries.insert(ordinal, record.clone());
        self.all_entries.insert((record.scope.clone(), ordinal), record);
        Ok(ReceiptDecision::Execute)
    }

    fn decision_for_existing(
        &mut self,
        call: &CompletedToolCall,
        ordinal: u32,
        digest: &str,
        existing: ReceiptRecord,
    ) -> Result<ReceiptDecision, DeveloperLoopError> {
        if existing.tool != call.name().as_str() || existing.request_sha256 != digest {
            return Ok(ReceiptDecision::Refuse {
                detail: format!(
                    "effect receipt conflict at {} effect {}: the recovered request differs from the durably started request",
                    self.scope, ordinal
                ),
                ambiguous: false,
            });
        }
        match existing.state {
            ReceiptState::Completed => Ok(ReceiptDecision::Replay {
                value: existing.output.ok_or_else(|| tool("completed receipt lost its result"))?,
                is_error: existing
                    .is_error
                    .ok_or_else(|| tool("completed receipt lost its result status"))?,
            }),
            ReceiptState::Applied => Ok(ReceiptDecision::RecoverCheckpoint {
                value: existing.output.ok_or_else(|| tool("applied receipt lost its result"))?,
                is_error: existing
                    .is_error
                    .ok_or_else(|| tool("applied receipt lost its result status"))?,
            }),
            ReceiptState::Reviewed => Ok(ReceiptDecision::Replay {
                value: existing.output.ok_or_else(|| tool("reviewed receipt lost its result"))?,
                is_error: existing
                    .is_error
                    .ok_or_else(|| tool("reviewed receipt lost its result status"))?,
            }),
            ReceiptState::Started if command_effect(call.name().as_str()) => {
                if let Some(owner) = existing.native_owner {
                    return Ok(ReceiptDecision::RecoverCommandOwner {
                        owner,
                        scope: existing.scope,
                        ordinal,
                    });
                }
                let record = ReceiptRecord { state: ReceiptState::Ambiguous, ..existing };
                self.append(&record)?;
                self.entries.insert(ordinal, record.clone());
                self.all_entries.insert((record.scope.clone(), ordinal), record.clone());
                Ok(ReceiptDecision::Refuse {
                    detail: ambiguous(&self.scope, ordinal, &record.call_id),
                    ambiguous: true,
                })
            }
            ReceiptState::Ambiguous if command_effect(call.name().as_str()) => {
                if let Some(owner) = existing.native_owner {
                    return Ok(ReceiptDecision::RecoverCommandOwner {
                        owner,
                        scope: existing.scope,
                        ordinal,
                    });
                }
                Ok(ReceiptDecision::Refuse {
                    detail: ambiguous(&self.scope, ordinal, &existing.call_id),
                    ambiguous: true,
                })
            }
            ReceiptState::Started => Ok(ReceiptDecision::Execute),
            ReceiptState::Ambiguous => Ok(ReceiptDecision::Refuse {
                detail: ambiguous(&self.scope, ordinal, &existing.call_id),
                ambiguous: true,
            }),
        }
    }

    pub(super) fn complete(
        &mut self,
        value: &Value,
        is_error: bool,
    ) -> Result<(), DeveloperLoopError> {
        self.applied(value, is_error)?;
        self.finalize()
    }

    pub(super) fn applied(
        &mut self,
        value: &Value,
        is_error: bool,
    ) -> Result<(), DeveloperLoopError> {
        let ordinal = self.next_ordinal;
        let existing = self
            .entries
            .get(&ordinal)
            .cloned()
            .ok_or_else(|| tool("effect completed without a started receipt"))?;
        if !matches!(existing.state, ReceiptState::Started) {
            return Err(tool("effect receipt is not awaiting completion"));
        }
        let record = ReceiptRecord {
            state: ReceiptState::Applied,
            output: Some(value.clone()),
            is_error: Some(is_error),
            ..existing
        };
        self.append(&record)?;
        self.entries.insert(ordinal, record.clone());
        self.all_entries.insert((record.scope.clone(), ordinal), record);
        Ok(())
    }

    pub(super) fn finalize(&mut self) -> Result<(), DeveloperLoopError> {
        let ordinal = self.next_ordinal;
        let existing = self
            .entries
            .get(&ordinal)
            .cloned()
            .ok_or_else(|| tool("effect finalized without an applied receipt"))?;
        if !matches!(existing.state, ReceiptState::Applied) {
            return Err(tool("effect receipt is not awaiting checkpoint finalization"));
        }
        let record = ReceiptRecord { state: ReceiptState::Completed, ..existing };
        self.append(&record)?;
        self.entries.insert(ordinal, record.clone());
        self.all_entries.insert((record.scope.clone(), ordinal), record);
        Ok(())
    }

    fn prior_command_barrier(
        &self,
        call: &CompletedToolCall,
        request_sha256: &str,
    ) -> Result<Option<ReceiptDecision>, DeveloperLoopError> {
        if call.name().as_str() == "command_cancel"
            && serde_json::from_slice::<Value>(call.arguments().canonical_bytes())
                .ok()
                .and_then(|arguments| {
                    arguments.get("handle").and_then(Value::as_str).map(str::to_owned)
                })
                .is_some_and(|handle| {
                    self.all_entries.values().any(|record| {
                        same_receipt_epoch(&record.scope, &self.scope)
                            && matches!(
                                record.state,
                                ReceiptState::Started
                                    | ReceiptState::Ambiguous
                                    | ReceiptState::Reviewed
                            )
                            && record
                                .native_owner
                                .is_some_and(|owner| native_action_hex(owner.action) == handle)
                    })
                })
        {
            // The explicit cancellation tool will create its own receipt and bind the exact
            // retained owner before signaling it. Other effects remain fenced by the barrier.
            return Ok(None);
        }
        let Some(record) = self.all_entries.values().find(|record| {
            record.scope != self.scope
                && same_receipt_epoch(&record.scope, &self.scope)
                && command_effect(&record.tool)
                && matches!(
                    record.state,
                    ReceiptState::Started | ReceiptState::Ambiguous | ReceiptState::Reviewed
                )
        }) else {
            return Ok(None);
        };
        match record.state {
            ReceiptState::Reviewed
                if record.tool == call.name().as_str()
                    && record.request_sha256 == request_sha256 =>
            {
                Ok(Some(ReceiptDecision::Replay {
                    value: record
                        .output
                        .clone()
                        .ok_or_else(|| tool("reviewed receipt lost its result"))?,
                    is_error: record
                        .is_error
                        .ok_or_else(|| tool("reviewed receipt lost its result status"))?,
                }))
            }
            ReceiptState::Reviewed => Ok(Some(ReceiptDecision::Refuse {
                detail: "new mutating effects are blocked because a command outcome in this requirements revision remains unknown; inspect and preserve the retained state, or ask the user for new input before changing it"
                    .to_owned(),
                ambiguous: true,
            })),
            ReceiptState::Started | ReceiptState::Ambiguous => Ok(Some(ReceiptDecision::Refuse {
                detail: ambiguous(&record.scope, record.ordinal, &record.call_id),
                ambiguous: true,
            })),
            ReceiptState::Applied | ReceiptState::Completed => {
                unreachable!("filtered receipt state")
            }
        }
    }
}

/// Inspects every exact command owner in the latest receipt frames without repairing a partial
/// tail or changing any receipt state.
pub(super) fn receipt_linked_native_owners(
    path: &Path,
) -> Result<Vec<NativeCommandOwner>, DeveloperLoopError> {
    let mut owners = Vec::new();
    for record in storage::inspect(path)? {
        if command_effect(&record.tool)
            && let Some(owner) = record.native_owner
            && !owners.contains(&owner)
        {
            owners.push(owner);
        }
    }
    Ok(owners)
}

fn invocation_epoch(scope: &str) -> &str {
    scope.rsplit_once("-invocation-").map_or(scope, |(epoch, _)| epoch)
}

fn same_receipt_epoch(left: &str, right: &str) -> bool {
    match (run_revision(left), run_revision(right)) {
        (Some(left), Some(right)) => left == right,
        _ => invocation_epoch(left) == invocation_epoch(right),
    }
}

fn run_revision(scope: &str) -> Option<(&str, &str)> {
    let value = scope.strip_prefix("peritus-")?;
    let run = value.get(..32)?;
    let remainder = value.get(32..)?;
    if !run.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let (_, revision_and_invocation) = remainder.rsplit_once("-revision-")?;
    let (revision, _) = revision_and_invocation.split_once("-invocation-")?;
    if revision.is_empty() || !revision.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((run, revision))
}

fn receipt_revision(scope: &str) -> Option<u64> {
    run_revision(scope)?.1.parse().ok()
}

fn request_digest(call: &CompletedToolCall) -> String {
    let mut hasher = Sha256::new();
    hasher.update(call.name().as_str().as_bytes());
    hasher.update([0]);
    hasher.update(call.arguments().canonical_bytes());
    hex(hasher.finalize().into())
}

fn hex(bytes: [u8; 32]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn ambiguous(scope: &str, ordinal: u32, call_id: &str) -> String {
    format!(
        "ambiguous prior command outcome at {scope} effect {ordinal} (provider call {call_id}); Peritus will not run it again automatically because the command may already have taken effect"
    )
}

fn command_effect(tool: &str) -> bool {
    matches!(
        tool,
        "run_command"
            | "command_start"
            | "command_stdin"
            | "command_resize"
            | "command_signal"
            | "command_cancel"
    )
}

fn native_action_hex(action_id: ActionId) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(32);
    for byte in action_id.as_bytes() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn fields_are_consistent(record: &ReceiptRecord) -> bool {
    if record.owner_inactive && record.native_owner.is_none() {
        return false;
    }
    match record.state {
        ReceiptState::Started | ReceiptState::Ambiguous => {
            record.output.is_none() && record.is_error.is_none()
        }
        ReceiptState::Applied | ReceiptState::Completed => {
            record.output.is_some() && record.is_error.is_some()
        }
        ReceiptState::Reviewed => record.output.is_some() && record.is_error == Some(true),
    }
}

#[cfg(test)]
#[path = "receipt/tests.rs"]
mod tests;
