//! Strict task-level developer terminal decoding.

use serde::Deserialize;

use peritus_tools_shell::ExecInput;

use crate::model_output::{TypedObjectError, last_typed_object};
use crate::{ProductDeliveryScope, ProductRunnerError, ProductRunnerErrorKind};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalWire {
    kind: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    run_instructions: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    capabilities: Option<Vec<crate::delivery::Capability>>,
    #[serde(default)]
    handoff: Option<crate::delivery::DraftHandoff>,
}

pub(super) enum TerminalTurn {
    Complete((String, String)),
    Question(String),
    Plan(Vec<crate::delivery::Capability>),
    Draft(crate::delivery::DraftHandoff),
}

pub(super) fn parse(value: &str) -> Result<TerminalTurn, ProductRunnerError> {
    let wire: TerminalWire = match last_typed_object(value) {
        Ok(wire) => wire,
        Err(TypedObjectError::Missing) => {
            return Err(invalid("developer response contains no JSON"));
        }
        Err(TypedObjectError::Invalid(detail)) => {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "parse developer terminal",
                detail,
            ));
        }
    };
    match (
        wire.kind.as_str(),
        wire.summary,
        wire.run_instructions,
        wire.message,
        wire.capabilities,
        wire.handoff,
    ) {
        ("complete", Some(summary), Some(run_instructions), None, None, None)
            if !summary.trim().is_empty() && !run_instructions.trim().is_empty() =>
        {
            Ok(TerminalTurn::Complete((summary, run_instructions)))
        }
        ("question", None, None, Some(message), None, None) if !message.trim().is_empty() => {
            Ok(TerminalTurn::Question(message))
        }
        ("plan", None, None, None, Some(plan), None) => Ok(TerminalTurn::Plan(plan)),
        ("draft", None, None, None, None, Some(handoff)) => Ok(TerminalTurn::Draft(handoff)),
        _ => Err(invalid("developer terminal fields do not match its kind")),
    }
}

pub(super) fn validate_run_instructions(
    scope: ProductDeliveryScope,
    terminal: TerminalTurn,
) -> Result<TerminalTurn, ProductRunnerError> {
    if let (ProductDeliveryScope::WorkspaceChanges, TerminalTurn::Complete((_, command))) =
        (scope, &terminal)
    {
        ExecInput::from_command_line(command).map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InvalidModelOutput,
                "validate candidate run command",
                error.detail(),
            )
        })?;
    }
    Ok(terminal)
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate developer terminal",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_plans_and_draft_debt_remain_distinct_from_task_completion() {
        let plan = r#"{"kind":"plan","capabilities":[{"id":"open","requirements":["r1"],"source_quote":"OPEN","outcome":"open a file","depends_on":[],"kind":"behavior","positive":"valid OPEN returns a real handle","negative":"invalid OPEN preserves state"}]}"#;
        assert!(matches!(parse(plan).unwrap(), TerminalTurn::Plan(items) if items.len() == 1));
        let draft = r#"{"kind":"draft","handoff":{"capability":"open","implementation":"server/open.rs","remaining":["still returns Unsupported"]}}"#;
        assert!(
            matches!(parse(draft).unwrap(), TerminalTurn::Draft(handoff) if handoff.remaining.len() == 1)
        );
        assert!(parse(r#"{"kind":"complete","summary":"done","run_instructions":"cargo run","handoff":{"capability":"open","implementation":"server/open.rs","remaining":[]}}"#).is_err());
        assert!(parse(r#"{"kind":"draft","handoff":{"capability":"open","implementation":"server/open.rs","remaining":[],"verified":true}}"#).is_err());
    }
}
