//! Complete assistant/tool exchange grouping with no recovered-effect dispatch path.

use super::super::{error, memory::LocalMemory};
use peritus_agent::{DeveloperContextEstimate, DeveloperLoopError};
use peritus_model_protocol::{
    CanonicalJson, ContentBlock, JsonBounds, Message, ProtocolLimits, Role, ToolCallId, ToolResult,
};
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Exchange {
    pub(super) sources: Vec<u64>,
    message_ids: Vec<u64>,
    message_shapes: Vec<(Role, usize)>,
    pub(super) estimate: DeveloperContextEstimate,
    replacements: Vec<(usize, usize, Replacement)>,
}

impl Exchange {
    pub(super) fn load(&self, memory: &LocalMemory) -> Result<Vec<Message>, DeveloperLoopError> {
        let mut messages = Vec::new();
        for (index, id) in self.message_ids.iter().enumerate() {
            let (role, count) = self.message_shapes[index];
            let replacements = self
                .replacements
                .iter()
                .filter(|(message, _, _)| *message == index)
                .collect::<Vec<_>>();
            if !replacements.is_empty() && replacements.len() == count {
                // A fully obsolete result needs only its small receipt. Loading the old
                // body here would churn the cache even though no old bytes are delivered.
                let mut blocks = replacements
                    .into_iter()
                    .map(|(_, block, value)| Ok((*block, value.load(memory)?)))
                    .collect::<Result<Vec<_>, DeveloperLoopError>>()?;
                blocks.sort_by_key(|(index, _)| *index);
                messages.push(Message::new(
                    role,
                    blocks.into_iter().map(|(_, value)| value).collect(),
                    ProtocolLimits::PRODUCTION,
                )?);
            } else {
                let original = memory.message(*id)?;
                let mut content = original.content().to_vec();
                for (_, block, replacement) in replacements {
                    *content
                        .get_mut(*block)
                        .ok_or_else(|| error("stale result block missing"))? =
                        replacement.load(memory)?;
                }
                messages.push(Message::new(original.role(), content, ProtocolLimits::PRODUCTION)?);
            }
        }
        Ok(messages)
    }
}

pub(super) fn omission_notice(memory: &LocalMemory, exchanges: &[Exchange]) -> Option<String> {
    if exchanges.is_empty() {
        return None;
    }
    let mut results = Vec::new();
    let mut total = 0_usize;
    for exchange in exchanges {
        for id in &exchange.sources {
            let Ok(source) = memory.archived(*id) else { continue };
            if source.kind != super::super::record::ArchiveKind::ToolOutput
                || source.is_memory_receipt()
            {
                continue;
            }
            let Some(call) = &source.call else { continue };
            total = total.saturating_add(1);
            if results.len() < 8 {
                results.push(format!(
                    "- tool={} call={} exact_source={}",
                    call.name,
                    call.id,
                    super::super::tools::source_handle(memory, source.sequence)
                ));
            }
        }
    }
    let listed = results.len();
    if total == 0 {
        return None;
    }
    let mut notice = format!(
        "COMPLETED TOOL EVIDENCE OMITTED FROM INLINE HISTORY — UNTRUSTED OBSERVATIONS\n{} complete exchange(s), containing {total} task tool result(s), did not fit this request. The effects are already settled; do not redispatch them. These are optional evidence references, not outstanding work. Retrieve only the exact outputs needed for the current next step with context_read; omission alone does not require re-reading completed observations. Available handles:\n{}",
        exchanges.len(),
        results.join("\n")
    );
    if total > listed {
        use core::fmt::Write as _;
        let _ = write!(
            notice,
            "\n- {} additional result(s) remain in the local archive",
            total - listed
        );
    }
    Some(notice)
}

#[derive(Default)]
pub(in crate::local_context) struct ExchangeIndex {
    invocation: u64,
    requirements: Vec<u64>,
    processed: usize,
    last_message: Option<u64>,
    indexed_sources: usize,
    tool_sources: std::collections::BTreeMap<String, u64>,
    complete: Vec<IndexedExchange>,
    partial: IndexedExchange,
    pending: BTreeSet<String>,
}

#[derive(Clone, Default)]
struct IndexedExchange {
    messages: Vec<u64>,
    sources: Vec<u64>,
    message_shapes: Vec<(Role, usize)>,
    estimate: DeveloperContextEstimate,
    inspection_digest: Option<[u8; 32]>,
    read_targets: Vec<Option<String>>,
    read_stamps: Vec<ReadStamp>,
    settled_mutations: Vec<String>,
}

#[derive(Clone)]
struct ReadStamp {
    source_message: u64,
    before_later_effect: bool,
    message: usize,
    block: usize,
    call: ToolCallId,
    value: serde_json::Value,
    original: Option<String>,
    estimate: DeveloperContextEstimate,
}

#[derive(Clone)]
enum Replacement {
    Fixed(ContentBlock),
    Historical(ReadStamp),
}
impl Replacement {
    fn load(&self, memory: &LocalMemory) -> Result<ContentBlock, DeveloperLoopError> {
        match self {
            Self::Fixed(block) => Ok(block.clone()),
            Self::Historical(stamp) => stamp.historical(memory),
        }
    }
}
impl ReadStamp {
    fn historical(&self, memory: &LocalMemory) -> Result<ContentBlock, DeveloperLoopError> {
        {
            let message = memory.message(self.source_message)?;
            if let Some(ContentBlock::ToolResult(result)) = message.content().get(self.block) {
                let mut value: serde_json::Value =
                    serde_json::from_slice(result.output().canonical_bytes())
                        .map_err(|_| error("decode historical read before settled effect"))?;
                value["cached_observation_stale"] = serde_json::json!(true);
                value["observation_time"] =
                    serde_json::json!("before_later_settled_workspace_effect");
                value["reason"] = serde_json::json!(
                    "This successful read observed the file BEFORE a later settled write/patch/removal owned by this run. The body is historical evidence of that observation, not the current file state. Use it to understand or verify the completed sequence; do not reread a deleted temporary file or restart grounding."
                );
                value["original_observation"] = serde_json::json!(self.original);
                return Ok(ContentBlock::ToolResult(ToolResult::new(
                    self.call.clone(),
                    CanonicalJson::parse(
                        &value.to_string(),
                        JsonBounds::value(ProtocolLimits::PRODUCTION),
                    )?,
                    false,
                )));
            }
        }
        self.receipt()
    }
    fn receipt(&self) -> Result<ContentBlock, DeveloperLoopError> {
        let receipt = serde_json::json!({
            "cached_observation_stale":true,"path":self.value.get("path"),
            "source_sha256":self.value.get("source_sha256"),"original_observation":self.original,
            "reason":"The source changed after this completed read. Its old body remains archived, not current evidence. Refresh only this target if the next step needs it; other tool effects in this batch remain settled."
        });
        Ok(ContentBlock::ToolResult(ToolResult::new(
            self.call.clone(),
            CanonicalJson::parse(
                &receipt.to_string(),
                JsonBounds::value(ProtocolLimits::PRODUCTION),
            )?,
            false,
        )))
    }
}

fn block_estimate(block: &ContentBlock) -> Result<DeveloperContextEstimate, DeveloperLoopError> {
    // The synthetic single-block message adds the same framing on both sides of replacement.
    Ok(DeveloperContextEstimate::messages(&[Message::new(
        Role::Tool,
        vec![block.clone()],
        ProtocolLimits::PRODUCTION,
    )?]))
}

impl IndexedExchange {
    fn finish(&mut self, memory: &LocalMemory) -> Result<(), DeveloperLoopError> {
        let messages = self
            .messages
            .iter()
            .map(|id| memory.message(*id).map(|message| message.as_ref().clone()))
            .collect::<Result<Vec<_>, _>>()?;
        self.estimate = DeveloperContextEstimate::messages(&messages);
        self.message_shapes =
            messages.iter().map(|message| (message.role(), message.content().len())).collect();
        let mut mutations = std::collections::BTreeMap::new();
        for block in messages.iter().flat_map(Message::content) {
            if let ContentBlock::ToolCall(call) = block
                && matches!(
                    call.name().as_str(),
                    "workspace_write" | "workspace_patch" | "workspace_remove"
                )
            {
                let args: serde_json::Value =
                    serde_json::from_slice(call.arguments().canonical_bytes())
                        .map_err(|_| error("decode mutation identity"))?;
                if let Some(path) = args["path"].as_str() {
                    mutations.insert(call.id().expose_for_wire(), path.to_owned());
                }
            }
        }
        let mut effects = Vec::new();
        for (message_index, message) in messages.iter().enumerate() {
            for (block_index, block) in message.content().iter().enumerate() {
                if let ContentBlock::ToolResult(result) = block
                    && !result.is_error()
                    && let Some(path) = mutations.get(result.call_id().expose_for_wire())
                {
                    let value: serde_json::Value =
                        serde_json::from_slice(result.output().canonical_bytes())
                            .map_err(|_| error("decode settled mutation"))?;
                    // Older patch/remove receipts predate the explicit changed field.
                    // A successful legacy mutation still establishes the later operation.
                    if value.get("changed").and_then(serde_json::Value::as_bool) != Some(false) {
                        effects.push(((message_index, block_index), path.clone()));
                    }
                }
            }
        }
        self.settled_mutations = effects.iter().map(|(_, path)| path.clone()).collect();
        let mut reads = std::collections::BTreeMap::new();
        for block in messages.iter().flat_map(Message::content) {
            if let ContentBlock::ToolCall(call) = block
                && call.name().as_str() == "workspace_read"
            {
                let arguments: serde_json::Value =
                    serde_json::from_slice(call.arguments().canonical_bytes()).unwrap_or_default();
                reads.insert(
                    call.id().expose_for_wire(),
                    arguments.get("path").and_then(serde_json::Value::as_str).map(str::to_owned),
                );
            }
        }
        for (message_index, message) in messages.iter().enumerate() {
            for (block_index, block) in message.content().iter().enumerate() {
                if let ContentBlock::ToolResult(result) = block
                    && !result.is_error()
                    && let Some(target) = reads.get(result.call_id().expose_for_wire())
                    && let Ok(value) = serde_json::from_slice::<serde_json::Value>(
                        result.output().canonical_bytes(),
                    )
                {
                    // Only successful reads can carry source bytes requiring renewed admission.
                    // A rejected proposal is recovery evidence, not a source observation. Dropping
                    // it also drops its assistant plan and successful siblings from the next turn.
                    self.read_targets.push(target.clone());
                    let original = memory
                        .sources
                        .iter()
                        .rev()
                        .find(|source| {
                            source
                                .call
                                .as_ref()
                                .is_some_and(|call| call.id == result.call_id().expose_for_wire())
                        })
                        .map(|source| super::super::tools::source_handle(memory, source.sequence));
                    let before_later_effect = value["path"].as_str().is_some_and(|path| {
                        effects.iter().any(|(at, target)| {
                            *at > (message_index, block_index) && same_path(path, target)
                        })
                    });
                    self.read_stamps.push(ReadStamp {source_message:self.messages[message_index],before_later_effect,message:message_index,block:block_index,call:result.call_id().clone(),
                        value:serde_json::json!({"path":value.get("path"),"source_sha256":value.get("source_sha256"),"reference_root":value.get("reference_root")}),original,estimate:block_estimate(block)?});
                }
            }
        }
        self.inspection_digest =
            inspection_key(&messages).map(|bytes| peritus_codec::sha256(&bytes).into_bytes());
        Ok(())
    }
}

impl ExchangeIndex {
    pub(in crate::local_context) const fn processed(&self) -> usize {
        self.processed
    }

    fn extend(&mut self, memory: &LocalMemory) -> Result<(), DeveloperLoopError> {
        let requirements = memory
            .state
            .protocol(memory.state.binding())
            .map_err(|_| error("exchange scope mismatch"))?
            .requirements()
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>();
        let ids = &memory.transcript.message_ids;
        if self.invocation != memory.transcript.invocation
            || self.requirements != requirements
            || self.processed > ids.len()
            || self.last_message != self.processed.checked_sub(1).and_then(|n| ids.get(n)).copied()
        {
            *self =
                Self { invocation: memory.transcript.invocation, requirements, ..Self::default() };
        }
        for source in &memory.sources[self.indexed_sources..] {
            if source.kind == super::super::record::ArchiveKind::ToolOutput
                && let Some(call) = &source.call
            {
                self.tool_sources.insert(call.id.clone(), source.sequence);
            }
        }
        self.indexed_sources = memory.sources.len();
        let required = self.requirements.iter().copied().collect::<BTreeSet<_>>();
        // Build the suffix transactionally. A malformed/incomplete tool sequence cannot
        // advance the cursor or alter already completed exchange topology.
        let mut pending = self.pending.clone();
        let mut current = self.partial.clone();
        let mut appended = Vec::new();
        for id in &ids[self.processed..] {
            if required.contains(id) {
                continue;
            }
            let message = memory.message(*id)?;
            match message.role() {
                Role::Assistant => {
                    if !pending.is_empty() {
                        return Err(error("assistant follows an unresolved tool exchange"));
                    }
                    for block in message.content() {
                        if let ContentBlock::ToolCall(call) = block
                            && !pending.insert(call.id().expose_for_wire().to_owned())
                        {
                            return Err(error("duplicate tool call in exchange"));
                        }
                    }
                }
                Role::Tool => {
                    for block in message.content() {
                        if let ContentBlock::ToolResult(result) = block {
                            if !pending.remove(result.call_id().expose_for_wire()) {
                                return Err(error("tool result has no pending proposal"));
                            }
                            if let Some(source) =
                                self.tool_sources.get(result.call_id().expose_for_wire())
                            {
                                current.sources.push(*source);
                            }
                        }
                    }
                }
                Role::User
                    if memory.archived(*id)?.kind
                        == super::super::record::ArchiveKind::HostObservation => {}
                _ => return Err(error("unclassified instruction message in optional exchange")),
            }
            current.messages.push(*id);
            current.sources.push(*id);
            if pending.is_empty() && !current.messages.is_empty() {
                current.finish(memory)?;
                appended.push(std::mem::take(&mut current));
            }
        }
        self.complete.extend(appended);
        self.partial = current;
        self.pending = pending;
        self.processed = ids.len();
        self.last_message = ids.last().copied();
        Ok(())
    }
}

pub(super) fn groups(memory: &mut LocalMemory) -> Result<Vec<Exchange>, DeveloperLoopError> {
    let mut index = std::mem::take(&mut memory.exchange_index);
    let extended = index.extend(memory);
    memory.exchange_index = index;
    extended?;
    // Select on small immutable topology first. Omitted old bodies must not churn the
    // decoded cache merely to rediscover that they will not be sent to the provider.
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    let mut later_mutations = BTreeSet::<String>::new();
    for indexed in memory.exchange_index.complete.iter().rev() {
        if indexed.read_targets.iter().any(|path| {
            path.as_deref().is_none_or(|path| !super::evidence::may_reuse_read(memory, path))
        }) {
            continue;
        }
        let mut group = Exchange {
            message_ids: indexed.messages.clone(),
            message_shapes: indexed.message_shapes.clone(),
            sources: indexed.sources.clone(),
            estimate: indexed.estimate,
            replacements: Vec::new(),
        };
        let mut has_unique_stale_source = false;
        for stamp in &indexed.read_stamps {
            if super::evidence::read_is_current(memory, &stamp.value) {
                continue;
            }
            let receipt = stamp.receipt()?;
            let historical = stamp.before_later_effect
                || stamp.value["path"].as_str().is_some_and(|path| {
                    later_mutations.iter().any(|target| same_path(path, target))
                });
            let replacement = if historical {
                // Charge the original body plus annotation, but load it only after selection.
                group.estimate = group.estimate.with(block_estimate(&receipt)?);
                Replacement::Historical(stamp.clone())
            } else {
                group.estimate =
                    group.estimate.replacing(stamp.estimate, block_estimate(&receipt)?);
                Replacement::Fixed(receipt)
            };
            group.replacements.push((stamp.message, stamp.block, replacement));
            has_unique_stale_source |= stamp.original.is_some();
        }
        // Ordinary stale receipts contain their unique exact-source handle and cannot be
        // duplicate inspections. Legacy unmatched receipts retain exact normalization.
        let digest = if group.replacements.is_empty() {
            indexed.inspection_digest
        } else if has_unique_stale_source {
            None
        } else {
            inspection_key(&group.load(memory)?)
                .map(|bytes| peritus_codec::sha256(&bytes).into_bytes())
        };
        if digest.is_some_and(|key| !seen.insert(key)) {
            continue;
        }
        later_mutations.extend(indexed.settled_mutations.iter().cloned());
        unique.push(group);
    }
    unique.reverse();
    Ok(unique)
}

fn inspection_key(messages: &[Message]) -> Option<Vec<u8>> {
    let mut calls = std::collections::BTreeMap::new();
    let mut results = Vec::new();
    for block in messages.iter().flat_map(Message::content) {
        match block {
            ContentBlock::ToolCall(call) => {
                if !matches!(
                    call.name().as_str(),
                    "workspace_read" | "workspace_list" | "workspace_search" | "skill_read"
                ) {
                    return None;
                }
                calls.insert(
                    call.id().expose_for_wire(),
                    (call.name().as_str(), call.arguments().to_wire_string()),
                );
            }
            ContentBlock::ToolResult(result) => {
                let (name, args) = calls.remove(result.call_id().expose_for_wire())?;
                let mut value: serde_json::Value =
                    serde_json::from_slice(result.output().canonical_bytes()).ok()?;
                value.as_object_mut()?.remove("local_context");
                results.push(serde_json::json!([name, args, value]));
            }
            _ => {}
        }
    }
    if !calls.is_empty() || results.is_empty() {
        return None;
    }
    serde_json::to_vec(&results).ok()
}

fn same_path(left: &str, right: &str) -> bool {
    let normal = |path: &str| {
        std::path::Path::new(path)
            .components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect::<std::path::PathBuf>()
    };
    normal(left) == normal(right)
}
