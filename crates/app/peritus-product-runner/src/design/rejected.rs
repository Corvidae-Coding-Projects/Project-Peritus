//! Exact, private, unapproved proposals. Repair never edits the accepted predecessor.

use peritus_model_protocol::{CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{
    SourceDesignSnapshot,
    expert_pass::{ExpertPass, failure},
};
use crate::{ProductRunInput, ProductRunnerError};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    schema: u32,
    authority: String,
    run: [u8; 16],
    input_revision: u64,
    source_sha256: String,
    draft_version: u64,
    tool: String,
    arguments: Value,
}

fn admitted(
    input: &ProductRunInput,
) -> Result<(SourceDesignSnapshot, ExpertPass), ProductRunnerError> {
    crate::execution::check_cancelled(input)?;
    if !input
        .conversation
        .effective_permissions()
        .allows(crate::control::PermissionCapability::Read)
    {
        return Err(failure("proposal recovery requires current read permission"));
    }
    let source = SourceDesignSnapshot::open(&input.trace_path, input.run_id, input.workspace_id)?;
    let pass = ExpertPass::load(input, &source)?;
    Ok((source, pass))
}

fn repairable(name: &str) -> bool {
    matches!(
        name,
        "design_amendment" | "design_research_plan" | "design_candidate_patch" | "design_publish"
    )
}

pub(super) fn retain(
    input: &ProductRunInput,
    call: &CompletedToolCall,
    path: &str,
    detail: &str,
) -> Result<Value, ProductRunnerError> {
    let mut error = json!({"code":"invalid_design_proposal","path":path,"detail":detail,"effect_executed":false,"accepted_checkpoint_changed":false});
    if repairable(call.name().as_str()) {
        let (_, pass) = admitted(input)?;
        let proposal = Proposal {
            schema: 1,
            authority: "unvalidated_proposal_only".into(),
            run: *input.run_id.as_bytes(),
            input_revision: pass.input_revision,
            source_sha256: pass.source,
            draft_version: pass.version,
            tool: call.name().as_str().to_owned(),
            arguments: serde_json::from_slice(call.arguments().canonical_bytes())
                .map_err(|e| failure(e.to_string()))?,
        };
        let bytes = serde_json::to_vec(&proposal).map_err(|e| failure(e.to_string()))?;
        let digest = crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes());
        super::retention::write(
            &input.trace_path.with_extension(format!("design-rejected-{digest}.json")),
            &bytes,
        )?;
        error["proposal"] = json!(digest);
        error["draft_version"] = json!(proposal.draft_version);
        error["source_sha256"] = json!(proposal.source_sha256);
        error["next_action"] = json!(
            "Use design_proposal_read only for needed rejected bytes, then design_proposal_repair with field replacements. design_pass_read records is the accepted predecessor, not this rejected proposal. Do not regenerate unchanged Markdown, evidence, or acceptance."
        );
        if proposal.arguments["source_sha256"] == proposal.source_sha256
            && proposal.arguments["expected_version"] == proposal.draft_version
        {
            super::retention::write(
                &input.trace_path.with_extension("design-rejected-latest.json"),
                &serde_json::to_vec(&error).map_err(|e| failure(e.to_string()))?,
            )?;
        } else {
            error["next_action"] = json!(
                "This rejected proposal has an invalid or stale binding. Read it as unapproved historical data, reconcile current requirements, and submit a newly bound call; repair cannot rebind old work."
            );
        }
    }
    Ok(json!({"error":error}))
}

fn load(input: &ProductRunInput, digest: &str) -> Result<Proposal, ProductRunnerError> {
    admitted(input)?;
    if digest.len() != 64
        || !digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(failure("invalid retained proposal handle"));
    }
    let bytes = super::retention::read(
        &input.trace_path.with_extension(format!("design-rejected-{digest}.json")),
    )?
    .ok_or_else(|| failure("retained proposal is unavailable"))?;
    if crate::encode_hex(peritus_codec::sha256(&bytes).as_bytes()) != digest {
        return Err(failure("retained proposal failed its content identity check"));
    }
    let proposal: Proposal = serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?;
    if proposal.schema != 1
        || proposal.authority != "unvalidated_proposal_only"
        || proposal.run != *input.run_id.as_bytes()
        || !repairable(&proposal.tool)
    {
        return Err(failure("retained proposal has a different owner or schema"));
    }
    Ok(proposal)
}

pub(super) fn read(input: &ProductRunInput, args: &Value) -> Result<Value, ProductRunnerError> {
    let digest = args["proposal"].as_str().ok_or_else(|| failure("proposal handle missing"))?;
    let proposal = load(input, digest)?;
    let text = serde_json::to_string(&proposal.arguments).map_err(|e| failure(e.to_string()))?;
    let offset = args["offset"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| failure("invalid proposal offset"))?;
    let count = args["max_bytes"]
        .as_u64()
        .filter(|v| (4..=32768).contains(v))
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| failure("invalid proposal page size"))?;
    if offset >= text.len() || !text.is_char_boundary(offset) {
        return Err(failure("proposal offset is outside the exact UTF-8 text"));
    }
    let end = text.floor_char_boundary(offset.saturating_add(count).min(text.len()));
    Ok(
        json!({"proposal":digest,"tool":proposal.tool,"authority":proposal.authority,"source_sha256":proposal.source_sha256,"draft_version":proposal.draft_version,"offset":offset,"next_offset":(end<text.len()).then_some(end),"total_bytes":text.len(),"text":&text[offset..end]}),
    )
}

pub(super) fn repair(
    input: &ProductRunInput,
    args: &Value,
) -> Result<CompletedToolCall, ProductRunnerError> {
    let digest = args["proposal"].as_str().ok_or_else(|| failure("proposal handle missing"))?;
    let mut proposal = load(input, digest)?;
    let (_, current) = admitted(input)?;
    if proposal.source_sha256 != current.source
        || proposal.draft_version != current.version
        || proposal.input_revision != input.conversation.incorporated_revision()
        || input.conversation.revision() != proposal.input_revision
        || args["source_sha256"] != current.source
        || args["expected_version"] != current.version
        || proposal.arguments["source_sha256"] != proposal.source_sha256
        || proposal.arguments["expected_version"] != proposal.draft_version
    {
        return Err(failure(
            "proposal repair is stale; reconcile the current source and accepted draft first",
        ));
    }
    let changes = args["changes"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 64)
        .ok_or_else(|| failure("proposal repair needs bounded field changes"))?;
    for change in changes {
        let path = change["path"]
            .as_str()
            .filter(|p| p.starts_with('/') && !p.is_empty())
            .ok_or_else(|| failure("repair needs an absolute JSON Pointer"))?;
        if ["/source_sha256", "/expected_version"]
            .iter()
            .any(|p| path == *p || path.starts_with(&format!("{p}/")))
        {
            return Err(failure("proposal repair cannot change its source or predecessor binding"));
        }
        let text =
            change["value_json"].as_str().ok_or_else(|| failure("replacement JSON missing"))?;
        let checked = CanonicalJson::parse(text, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|e| failure(e.to_string()))?;
        let replacement: Value = serde_json::from_slice(checked.canonical_bytes())
            .map_err(|e| failure(e.to_string()))?;
        replace(&mut proposal.arguments, path, replacement)?;
    }
    super::expert_tools::tool_call(&proposal.tool, &proposal.arguments)
}

pub(super) fn current(input: &ProductRunInput) -> Result<Option<Value>, ProductRunnerError> {
    let Some(bytes) =
        super::retention::read(&input.trace_path.with_extension("design-rejected-latest.json"))?
    else {
        return Ok(None);
    };
    let hint: Value = serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?;
    let handle =
        hint["proposal"].as_str().ok_or_else(|| failure("invalid rejected proposal index"))?;
    let proposal = load(input, handle)?;
    let (_, pass) = admitted(input)?;
    Ok((pass.source == proposal.source_sha256
        && pass.version == proposal.draft_version
        && pass.input_revision == proposal.input_revision)
        .then_some(hint))
}

fn replace(value: &mut Value, path: &str, replacement: Value) -> Result<(), ProductRunnerError> {
    for segment in path.split('/').skip(1) {
        let mut bytes = segment.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
                return Err(failure("repair pointer contains an invalid escape"));
            }
        }
    }
    let (parent, key) = path.rsplit_once('/').ok_or_else(|| failure("invalid repair pointer"))?;
    let key = key.replace("~1", "/").replace("~0", "~");
    let target = value.pointer_mut(parent).ok_or_else(|| failure("repair parent is missing"))?;
    if let Some(fields) = target.as_object_mut() {
        if fields.get(&key) == Some(&replacement) {
            return Err(failure("proposal repair has no change"));
        }
        fields.insert(key, replacement);
        return Ok(());
    }
    let index = key.parse::<usize>().map_err(|_| failure("invalid repair array index"))?;
    if key.len() > 1 && key.starts_with('0') {
        return Err(failure("repair array index is not canonical"));
    }
    let slot = target
        .as_array_mut()
        .and_then(|a| a.get_mut(index))
        .ok_or_else(|| failure("repair target is missing"))?;
    if *slot == replacement {
        return Err(failure("proposal repair has no change"));
    }
    *slot = replacement;
    Ok(())
}
