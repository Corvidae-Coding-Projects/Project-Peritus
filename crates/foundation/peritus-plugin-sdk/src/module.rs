//! Native module service contract carried by the existing framed plugin protocol.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ModuleHostRequest;

/// Response from an independently implemented native module coordinator.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModuleAction {
    /// Present a result or wait for the next explicit user command.
    Reply {
        /// Human-readable Markdown, pageable through the module's own commands.
        text: String,
        /// Explicit actions offered by this module, interpreted only after user selection.
        #[serde(default)]
        actions: Vec<ModuleMenuAction>,
        /// Optional private artifact explicitly selected for conversation/Design reuse.
        #[serde(default)]
        attachment: Option<ModuleAttachment>,
    },
    /// Publish durable progress without ending a user-selected long-running module.
    /// Requires a host supporting `checkpoint` and the `checkpoint_ack` service.
    Checkpoint {
        /// Stable publication identity; acknowledgement must echo it exactly.
        key: String,
        /// Human-readable Markdown retained in the conversation before acknowledgement.
        text: String,
        /// Actions available while the coordinator continues working.
        #[serde(default)]
        actions: Vec<ModuleMenuAction>,
    },
    /// Ask the host to execute one selected agent with borrowed host tools.
    Model {
        /// Durable module-owned request identity and instructions.
        request: ModuleAgentRequest,
    },
    /// Ask the host to perform one explicitly versioned service operation.
    HostService {
        /// Durable request envelope. Its contents do not grant authority.
        request: ModuleHostRequest,
    },
}
/// User-visible action supplied by executable module code.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleMenuAction {
    /// Plain-text action label.
    pub label: String,
    /// Arguments relative to `/module THIS_MODULE`; never a shell command.
    pub arguments: String,
    /// Whether selection submits immediately or prepares an editable draft.
    pub submit: bool,
}
/// Module-owned artifact offered for explicit conversation reuse.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAttachment {
    /// Plain-text source/report label.
    pub name: String,
    /// Relative regular file beneath this plugin session's private state directory.
    pub path: String,
}
/// One separately scoped agent request; the host owns inference and credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleAgentRequest {
    /// Stable identifier for resuming this exact stage, unique across module revisions.
    pub key: String,
    /// Existing model selection to use: writer or reviewer.
    pub role: String,
    /// Public progress label; no private model reasoning.
    pub status: String,
    /// Plugin-provided role instructions, subordinate to host policy and user authority.
    pub system: String,
    /// Focused task context. Further evidence is retrieved through tools.
    pub prompt: String,
    /// Optional provider access program required by this bounded model role.
    /// Hosts may honor this only through a compatible provider transport.
    #[serde(default)]
    pub cyber_access_program: Option<ModuleCyberAccessProgram>,
    /// Native host tool names, intersected with the installed package declaration.
    pub tools: Vec<String>,
    /// Additional functions implemented by this native Rust plugin.
    #[serde(default)]
    pub functions: Vec<ModuleFunction>,
}
/// Defensive-security access programs a native module may explicitly request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleCyberAccessProgram {
    /// `OpenAI` Daybreak Blue for legitimate defensive-security work.
    DaybreakBlue,
}
/// A model-callable function exported by a native Rust plugin.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleFunction {
    /// Stable tool identifier; cannot replace host-owned tool names.
    pub name: String,
    /// Purpose and input contract.
    pub description: String,
    /// Strict JSON input schema.
    pub schema: Value,
}
