//! Strict task-level developer terminal decoding.

use serde::Deserialize;

use peritus_tools_shell::ExecInput;

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
}

pub(super) enum TerminalTurn {
    Complete((String, String)),
    Question(String),
}

pub(super) fn parse(value: &str) -> Result<TerminalTurn, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid("developer response contains no JSON"))?;
    let end = value.rfind('}').ok_or_else(|| invalid("developer response has incomplete JSON"))?;
    let wire: TerminalWire = serde_json::from_str(&value[start..=end]).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse developer terminal",
            error.to_string(),
        )
    })?;
    match (wire.kind.as_str(), wire.summary, wire.run_instructions, wire.message) {
        ("complete", Some(summary), Some(run_instructions), None)
            if !summary.trim().is_empty() && !run_instructions.trim().is_empty() =>
        {
            Ok(TerminalTurn::Complete((summary, run_instructions)))
        }
        ("question", None, None, Some(message)) if !message.trim().is_empty() => {
            Ok(TerminalTurn::Question(message))
        }
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
