//! Checked structured argv and separately classified script inputs.

use peritus_process::CommandSpec;
use peritus_tool_protocol::BoundedJson;

use crate::{ShellError, ShellErrorKind};

const MAX_SCRIPT_BYTES: usize = 256 * 1_024;

/// Input accepted by `shell.exec`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecInput {
    executable: String,
    arguments: Vec<String>,
}

impl ExecInput {
    /// Validates literal argv and rejects command-string flags for known shell interpreters.
    ///
    /// # Errors
    /// Returns a typed failure for invalid C2 argv or an attempt to route a shell command string
    /// through the structured-argv operation.
    pub fn new(executable: impl Into<String>, arguments: Vec<String>) -> Result<Self, ShellError> {
        let executable = executable.into();
        CommandSpec::new(executable.clone(), arguments.clone())?;
        if is_shell_command_mode(&executable, &arguments) {
            return Err(ShellError::new(
                ShellErrorKind::InvalidInput,
                "shell command-string flags require the separately authorized shell.script tool",
            ));
        }
        Ok(Self { executable, arguments })
    }

    /// Parses one deliberately restricted direct-execution command into literal argv.
    ///
    /// This format is intended for persisted user-facing run instructions. It accepts
    /// whitespace-separated argv only: quoting, command separators, expansions, redirections,
    /// and multiple lines are rejected rather than interpreted.
    ///
    /// # Errors
    /// Returns a typed failure when the value is empty, contains command-language or markup
    /// syntax, starts with an environment assignment, or violates the structured-argv contract.
    pub fn from_command_line(value: &str) -> Result<Self, ShellError> {
        let value = value.trim();
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(invalid_direct_command());
        }
        if value.chars().any(is_command_language_character) {
            return Err(invalid_direct_command());
        }
        let mut words = value.split_ascii_whitespace();
        let executable = words.next().ok_or_else(invalid_direct_command)?;
        if executable.contains('=') {
            return Err(invalid_direct_command());
        }
        Self::new(executable, words.map(str::to_owned).collect())
    }

    /// Decodes already schema-validated protocol arguments defensively.
    ///
    /// # Errors
    /// Returns a typed failure if required properties have the wrong shape.
    pub fn from_arguments(arguments: &BoundedJson) -> Result<Self, ShellError> {
        let executable = string_property(arguments, "executable")?;
        let values = string_array_property(arguments, "arguments")?;
        Self::new(executable, values)
    }

    /// Returns the literal executable.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }

    /// Returns literal arguments in order.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) fn command(&self) -> Result<CommandSpec, ShellError> {
        CommandSpec::new(self.executable.clone(), self.arguments.clone()).map_err(Into::into)
    }
}

fn invalid_direct_command() -> ShellError {
    ShellError::new(
        ShellErrorKind::InvalidInput,
        "run instructions must be one direct command with whitespace-separated arguments and no quoting, expansion, redirection, markup, or shell operators",
    )
}

const fn is_command_language_character(character: char) -> bool {
    matches!(
        character,
        '\'' | '"' | '`' | '$' | '|' | '&' | ';' | '<' | '>' | '(' | ')' | '{' | '}' | '#'
    )
}

/// Explicit interpreter and script input accepted only by `shell.script`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptInput {
    interpreter: String,
    interpreter_arguments: Vec<String>,
    script: String,
    arguments: Vec<String>,
}

impl ScriptInput {
    /// Validates an explicit interpreter invocation.
    ///
    /// The script is inserted as one literal argument after `interpreter_arguments`; the caller
    /// therefore chooses an interpreter-specific command flag explicitly and no host shell parses
    /// or joins the values.
    ///
    /// # Errors
    /// Returns a typed failure for empty/oversized script text or invalid C2 argv.
    pub fn new(
        interpreter: impl Into<String>,
        interpreter_arguments: Vec<String>,
        script: impl Into<String>,
        arguments: Vec<String>,
    ) -> Result<Self, ShellError> {
        let interpreter = interpreter.into();
        let script = script.into();
        if script.is_empty() || script.len() > MAX_SCRIPT_BYTES || script.as_bytes().contains(&0) {
            return Err(ShellError::new(
                ShellErrorKind::InvalidInput,
                "script is empty, contains NUL, or exceeds 256 KiB",
            ));
        }
        let mut argv = interpreter_arguments.clone();
        argv.push(script.clone());
        argv.extend(arguments.iter().cloned());
        CommandSpec::new(interpreter.clone(), argv)?;
        Ok(Self { interpreter, interpreter_arguments, script, arguments })
    }

    /// Decodes already schema-validated protocol arguments defensively.
    ///
    /// # Errors
    /// Returns a typed failure if required properties have the wrong shape.
    pub fn from_arguments(arguments: &BoundedJson) -> Result<Self, ShellError> {
        Self::new(
            string_property(arguments, "interpreter")?,
            string_array_property(arguments, "interpreter_arguments")?,
            string_property(arguments, "script")?,
            string_array_property(arguments, "arguments")?,
        )
    }

    /// Returns the literal interpreter executable.
    #[must_use]
    pub fn interpreter(&self) -> &str {
        &self.interpreter
    }

    /// Returns interpreter arguments that precede the script argument.
    #[must_use]
    pub fn interpreter_arguments(&self) -> &[String] {
        &self.interpreter_arguments
    }

    /// Returns the literal script argument.
    #[must_use]
    pub fn script(&self) -> &str {
        &self.script
    }

    /// Returns arguments that follow the script argument.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    pub(crate) fn command(&self) -> Result<CommandSpec, ShellError> {
        let mut arguments = self.interpreter_arguments.clone();
        arguments.push(self.script.clone());
        arguments.extend(self.arguments.iter().cloned());
        CommandSpec::new(self.interpreter.clone(), arguments).map_err(Into::into)
    }
}

fn is_shell_command_mode(executable: &str, arguments: &[String]) -> bool {
    let name = executable.rsplit(['/', '\\']).next().unwrap_or(executable).to_ascii_lowercase();
    match name.as_str() {
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "csh" | "fish" => arguments
            .iter()
            .take_while(|argument| argument.as_str() != "--")
            .any(|argument| argument == "-c" || shell_option_contains_c(argument)),
        "cmd" | "cmd.exe" => arguments.iter().any(|argument| argument.eq_ignore_ascii_case("/c")),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => arguments.iter().any(|argument| {
            argument.eq_ignore_ascii_case("-command")
                || argument.eq_ignore_ascii_case("-encodedcommand")
                || argument.eq_ignore_ascii_case("-c")
        }),
        _ => false,
    }
}

fn shell_option_contains_c(argument: &str) -> bool {
    argument
        .strip_prefix('-')
        .is_some_and(|flags| !flags.starts_with('-') && flags.as_bytes().contains(&b'c'))
}

fn string_property(arguments: &BoundedJson, name: &str) -> Result<String, ShellError> {
    arguments.property(name).and_then(|value| value.as_str().map(str::to_owned)).ok_or_else(|| {
        ShellError::new(
            ShellErrorKind::InvalidInput,
            format!("required string property {name:?} is absent or invalid"),
        )
    })
}

fn string_array_property(arguments: &BoundedJson, name: &str) -> Result<Vec<String>, ShellError> {
    let values = arguments.property(name).and_then(|value| value.elements()).ok_or_else(|| {
        ShellError::new(
            ShellErrorKind::InvalidInput,
            format!("required array property {name:?} is absent or invalid"),
        )
    })?;
    values
        .into_iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                ShellError::new(
                    ShellErrorKind::InvalidInput,
                    format!("property {name:?} contains a non-string element"),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_argv_rejects_shell_command_mode() {
        let error = ExecInput::new("/bin/sh", vec!["-ec".into(), "touch escaped".into()])
            .expect_err("command strings must use shell.script");
        assert_eq!(error.kind(), ShellErrorKind::InvalidInput);
    }

    #[test]
    fn structured_argv_keeps_arguments_literal() {
        let input = ExecInput::new("printf", vec!["%s".into(), "$(touch escaped)".into()])
            .expect("literal argv");
        let command = input.command().expect("checked command");
        assert_eq!(command.arguments()[1], "$(touch escaped)");
    }

    #[test]
    fn direct_command_line_becomes_literal_argv() {
        let input = ExecInput::from_command_line("cargo run --quiet --features tui,serde")
            .expect("direct command");
        assert_eq!(input.executable(), "cargo");
        assert_eq!(input.arguments(), ["run", "--quiet", "--features", "tui,serde"]);
    }

    #[test]
    fn direct_command_line_rejects_shell_syntax_and_markup() {
        for value in [
            "cargo test && cargo run",
            "cargo run > output.txt",
            "cargo run `whoami`",
            "From the root, run `cargo run`.",
            "MODE=release cargo run",
            "cargo run\ncargo test",
            "sh -c echo",
        ] {
            let error = ExecInput::from_command_line(value).expect_err("restricted command");
            assert_eq!(error.kind(), ShellErrorKind::InvalidInput, "{value}");
        }
    }

    #[test]
    fn script_is_one_literal_argument() {
        let input = ScriptInput::new(
            "/bin/sh",
            vec!["-c".into()],
            "printf '%s' \"$1\"",
            vec!["peritus-script".into(), "value".into()],
        )
        .expect("script input");
        let command = input.command().expect("checked command");
        assert_eq!(command.arguments()[1], "printf '%s' \"$1\"");
    }
}
