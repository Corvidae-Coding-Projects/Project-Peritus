//! Versioned reconstruction identity. This never admits a request or reuses an old verdict.

use crate::{
    control::PermissionCapability,
    local_context::{error, memory::LocalMemory},
};
use peritus_agent::DeveloperLoopError;
use peritus_codec::sha256;
use peritus_model_protocol::{
    Message, ProtocolLimits, ProviderProfile, ToolDefinition, encode_messages,
};
use serde_json::json;

pub(super) fn record(
    memory: &LocalMemory,
    messages: &[Message],
    profile: &ProviderProfile,
    tools: &[ToolDefinition],
) -> Result<(), DeveloperLoopError> {
    if !peritus_provider_core::efficiency_diagnostics_enabled() {
        return Ok(());
    }
    let identity = identity(memory, messages, profile, tools)?;
    crate::efficiency::event(
        &memory.trace_path,
        json!({"event":"decision-reconstruction",
        "compiler_version":1,"key":crate::local_context::tools::hex(&identity),
        "semantics":"derived full-input view identity; always rebuilt; not provider admission or a cached verdict"}),
    );
    Ok(())
}

fn identity(
    memory: &LocalMemory,
    messages: &[Message],
    profile: &ProviderProfile,
    tools: &[ToolDefinition],
) -> Result<[u8; 32], DeveloperLoopError> {
    let view = encode_messages(messages, ProtocolLimits::PRODUCTION)?;
    let host = memory.conversation.as_deref();
    let revision = match host.and_then(crate::ConversationView::interaction) {
        Some(port) => port.input()?.revision,
        None => host.map_or_else(
            || memory.state.binding().conversation_revision(),
            crate::ConversationView::revision,
        ),
    };
    let environment = memory.state.environment();
    // Profile fields are all included. The private debug-field encoding is explicitly scoped
    // to compiler revision 1 and is not a wire format, cache grant, or persistent authority.
    let fields = json!({"compiler":1,"encoding":1,"lineage":memory.store.scope_digest().into_bytes(),
        "conversation_bound":memory.state.binding().conversation_revision(),"current_input":revision,
        "incorporated_input":host.map(crate::ConversationView::incorporated_revision),
        "permissions":host.map(|view| PermissionCapability::ALL.map(|capability| view.effective_permissions().allows(capability))),
        "reference_authority":host.map(|view| sha256(view.reference_authority_context().as_bytes()).into_bytes()),
        "protected":host.map(|view| sha256(format!("{:?}",view.protected_paths()).as_bytes()).into_bytes()),
        "candidate":environment.candidate().into_bytes(),
        "candidate_scope":if memory.workspace_scope.direct {"unknown_outside_scoped_inputs"} else {"whole_candidate"},
        "all_supplied_file_versions":memory.decision_files,
        "files":environment.files().iter().map(|file| (file.key().into_bytes(),file.digest().into_bytes())).collect::<Vec<_>>(),
        "pending":memory.transcript.pending,"configuration":memory.config,
        "profile":format!("{profile:?}"),"ordered_tools":crate::local_context::view_binding::tool_policy(tools)?,
        "source_root":memory.decision_index.root(),"sources":memory.decision_index.processed(),
        "exact_view":sha256(&view).into_bytes()});
    let bytes = serde_json::to_vec(&fields)
        .map_err(|_| error("encode decision reconstruction identity"))?;
    Ok(sha256(&bytes).into_bytes())
}

pub(super) fn record_request(
    memory: &LocalMemory,
    request: &peritus_model_protocol::ModelRequest,
) -> Result<(), DeveloperLoopError> {
    if !peritus_provider_core::efficiency_diagnostics_enabled() {
        return Ok(());
    }
    let profile =
        memory.profile.as_ref().ok_or_else(|| error("decision request lacks compiled profile"))?;
    let view_key = identity(memory, request.messages(), profile, request.tools())?;
    let fingerprint = request.fingerprint()?.digest().into_bytes();
    let encoded = encode_messages(request.messages(), ProtocolLimits::PRODUCTION)?;
    let matches = memory
        .last_checkpoint
        .as_ref()
        .is_some_and(|checkpoint| checkpoint.view.digest == sha256(&encoded));
    let key = sha256(
        &serde_json::to_vec(&json!([1, view_key, fingerprint]))
            .map_err(|_| error("encode complete decision request binding"))?,
    );
    crate::efficiency::event(
        &memory.trace_path,
        json!({"event":"decision-request-binding","compiler_version":1,
        "key":crate::local_context::tools::hex(key.as_bytes()),"request_fingerprint":crate::local_context::tools::hex(&fingerprint),
        "view_checkpoint_matches":matches,
        "semantics":"complete ModelRequest fingerprint includes options, reasoning, tool choice and ordered schemas; existing current-input admission still governs submission"}),
    );
    Ok(())
}
