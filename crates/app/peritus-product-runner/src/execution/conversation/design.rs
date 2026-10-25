//! Interview guidance and the only Design-mode mutation: its own private draft artifact.

use crate::{ProductRunInput, control::PermissionCapability, guided_design};
use peritus_agent::{DeveloperLoopError, DeveloperToolObservation};
use peritus_model_protocol::{
    BoundedText, CanonicalJson, CompletedToolCall, JsonBounds, JsonSchema, ProtocolLimits,
    SchemaDialect, ToolDefinition, ToolName,
};

pub(super) const POLICY: &str = r"You are Peritus in interactive Design mode. Help a person create a practical design document through a friendly conversational interview. They may be nontechnical. Your deliverable is a saved, editable Markdown design, not an implementation.

Start by briefly explaining: we will shape the idea together, you can change any answer, say 'not sure', ask to see the draft, or return later. Ask ONE focused question per reply, with two or three short suggested answers when useful and a clear invitation to answer in their own words. Explain a technical tradeoff in everyday language before asking the user to choose. Offer a sensible recommendation with its reason; do not ask the user to decide implementation minutiae you can resolve from repository evidence. Avoid long questionnaires, unnecessary ceremony, and repeated questions.

Work through these topics flexibly: goal and problem; who uses it; concrete journeys and examples; required behavior and explicit exclusions; constraints and existing behavior to preserve; a workable approach grounded in relevant source; acceptance through real operations. Reuse answers already supplied, skip irrelevant topics, and allow the user to jump ahead or back. If they are uncertain, record an explicit open question or clearly labeled assumption. Never invent agreement or shrink their ambition. Keep discovery proportional; a focused read is enough to ground a small change.

Maintain a running Markdown draft with a # title and sections for Goal, People and journeys, Requirements and scope, Approach, Acceptance through real use, and Open questions. Adapt/add sections when useful. Distinguish agreed requirements, proposed choices, and open questions. Preserve exact names, values, constraints, corrections, and acceptance examples. On the first substantive answer, save an initial draft. After each subsequent answer or edit, call design_draft to save the complete updated Markdown, interview stage, and the next question before saying it was saved. Use the exact current version as expected_version. If saving fails, say so and keep the unsaved text visible; never claim persistence without a successful tool result. The current draft and next question are supplied with each invocation, so resume from them without asking answered questions again.

Editing: accept natural-language changes, replacement sections, or a full replacement Markdown document. Update the saved draft, briefly describe what changed, and continue from the next unresolved choice. If asked to see/show/review the draft, use design_draft read and show the complete current document rather than summarizing it away. Never reset or replace an existing design merely because the user reopened Design mode.

When enough is known, save stage review and present a concise design review plus any material open questions. Ask what they would change or whether it is ready. If the user confirms the design, save stage ready and show the document's saved path. Explain that /design build starts implementation using this document; /design show displays it and further messages can revise it. Approval of the document does not itself authorize execution. If asked to implement while still in Design mode, explain the /design build action and keep this turn in Design mode.

Use only read-only workspace inspection and the design_draft tool. The draft tool writes only the private conversation artifact; it cannot edit project files, run commands, or start implementation. Do not claim to have implemented, built, tested, committed, or exported anything. Do not suggest unit tests or formal verification as a substitute for acceptance through real user operations.";

pub(super) fn definition() -> Result<ToolDefinition, DeveloperLoopError> {
    let limits = ProtocolLimits::PRODUCTION;
    Ok(ToolDefinition::new(
        ToolName::new("design_draft".to_owned())?,
        Some(BoundedText::new("Read or atomically update this conversation's saved Markdown design. This only saves a private draft, never changes workspace files or starts implementation. Read uses action=read with update=null. Update requires the complete document, current expected_version (0 initially), stage, and next_question (empty when ready). Save after incorporating each answer or edit.".to_owned(), limits)?),
        JsonSchema::parse(r#"{"type":"object","properties":{"action":{"type":"string","enum":["read","update"]},"update":{"anyOf":[{"type":"null"},{"type":"object","properties":{"expected_version":{"type":"integer","minimum":0},"stage":{"type":"string","enum":["goal","people","journeys","scope","approach","acceptance","review","ready"]},"next_question":{"type":"string"},"markdown":{"type":"string"}},"required":["expected_version","stage","next_question","markdown"],"additionalProperties":false}]}},"required":["action","update"],"additionalProperties":false}"#, SchemaDialect::Draft202012, JsonBounds::schema(limits))?,
        true,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    action: String,
    update: Option<guided_design::Update>,
}

pub(super) fn execute(
    input: &ProductRunInput,
    call: &CompletedToolCall,
) -> Result<DeveloperToolObservation, DeveloperLoopError> {
    let result = (|| -> Result<serde_json::Value, String> {
        if !input.conversation.effective_permissions().allows(PermissionCapability::Read) {
            return Err(
                "Design access is disabled by the current read permission; inspect /permissions"
                    .to_owned(),
            );
        }
        crate::execution::check_cancelled(input).map_err(|error| error.to_string())?;
        let arguments: Arguments = serde_json::from_slice(call.arguments().canonical_bytes())
            .map_err(|error| error.to_string())?;
        let draft = match (arguments.action.as_str(), arguments.update) {
            ("read", None) => guided_design::load(input).map_err(|error| error.to_string())?,
            ("update", Some(update)) => {
                Some(guided_design::update(input, update).map_err(|error| error.to_string())?)
            }
            _ => {
                return Err(
                    "Use action=read with update=null, or action=update with all draft fields"
                        .to_owned(),
                );
            }
        };
        Ok(draft.map_or_else(
            || serde_json::json!({"version":0,"markdown":null}),
            |draft| draft.observation(&guided_design::path(input)),
        ))
    })();
    let is_error = result.is_err();
    let value = result.unwrap_or_else(|error| serde_json::json!({"error":error}));
    Ok(DeveloperToolObservation {
        output: CanonicalJson::parse(
            &value.to_string(),
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )?,
        is_error,
    })
}
