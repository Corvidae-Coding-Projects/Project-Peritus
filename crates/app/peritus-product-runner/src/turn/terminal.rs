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
}

pub(super) enum TerminalTurn {
    Complete((String, String)),
    Question(String),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_terminal_object_is_selected_after_earlier_braced_prose() {
        let value = r#"Verified output {'policy_id': 'POLICY-2024-Q3'}.
The exact result is:
{"kind":"complete","summary":"Generated the policy rollup and report.","run_instructions":"cat out/summary.json"}"#;

        let terminal = parse(value).expect("final terminal");

        let TerminalTurn::Complete((summary, run_instructions)) = terminal else {
            panic!("expected completion");
        };
        assert_eq!(summary, "Generated the policy rollup and report.");
        assert_eq!(run_instructions, "cat out/summary.json");
    }

    #[test]
    fn terminal_schema_remains_strict_when_no_valid_terminal_object_exists() {
        let Err(error) = parse(
            r#"prose {"kind":"complete","summary":"done","run_instructions":"cargo test","extra":true}"#,
        ) else {
            panic!("unknown fields must remain rejected");
        };

        assert_eq!(error.kind(), ProductRunnerErrorKind::InvalidModelOutput);
        assert_eq!(error.operation(), "parse developer terminal");
    }
}
