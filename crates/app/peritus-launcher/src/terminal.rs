//! Small terminal boundary shared by line-oriented and guarded hidden input.

use std::io::{self, BufRead, Write};

use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::LauncherError;

pub fn product_title() -> Result<peritus_tui::TerminalTitle, LauncherError> {
    peritus_tui::TerminalTitle::acquire().map_err(|error| interaction(&error))
}

pub struct Terminal<'a> {
    input: Box<dyn BufRead + 'a>,
    output: Box<dyn Write + 'a>,
}

#[cfg(test)]
impl<'a> Terminal<'a> {
    pub(crate) fn for_test(input: impl BufRead + 'a, output: impl Write + 'a) -> Self {
        Self { input: Box::new(input), output: Box::new(output) }
    }
}

impl Terminal<'static> {
    pub fn stdio() -> Self {
        Self { input: Box::new(io::BufReader::new(io::stdin())), output: Box::new(io::stdout()) }
    }
}

impl Terminal<'_> {
    pub fn line(&mut self, message: &str) -> Result<(), LauncherError> {
        writeln!(self.output, "{message}").map_err(|error| interaction(&error))
    }

    pub fn prompt(&mut self, message: &str) -> Result<String, LauncherError> {
        write!(self.output, "{message}").map_err(|error| interaction(&error))?;
        self.output.flush().map_err(|error| interaction(&error))?;
        let mut answer = String::new();
        if self.input.read_line(&mut answer).map_err(|error| interaction(&error))? == 0 {
            return Err(input_ended("setup"));
        }
        let answer = answer.trim().to_owned();
        if answer.eq_ignore_ascii_case("q") {
            return Err(input_cancelled("setup"));
        }
        Ok(answer)
    }

    pub fn confirm(&mut self, message: &str, default: bool) -> Result<bool, LauncherError> {
        loop {
            let answer = self.prompt(message)?;
            if answer.is_empty() {
                return Ok(default);
            }
            match answer.to_ascii_lowercase().as_str() {
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => self.line("Enter yes or no.")?,
            }
        }
    }

    pub(crate) fn hidden_input<T>(
        &mut self,
        operation: impl FnOnce(&mut HiddenInput<'_>) -> Result<T, LauncherError>,
    ) -> Result<T, LauncherError> {
        let mut input = HiddenInput::acquire(self.output.as_mut())?;
        let result = operation(&mut input);
        input.finish(result)
    }
}

pub(crate) struct HiddenInput<'a> {
    output: &'a mut dyn Write,
    raw_mode: bool,
    bracketed_paste: bool,
}

impl<'a> HiddenInput<'a> {
    fn acquire(output: &'a mut dyn Write) -> Result<Self, LauncherError> {
        enable_raw_mode().map_err(|error| interaction(&error))?;
        let mut input = Self { output, raw_mode: true, bracketed_paste: false };

        // Claim cleanup before issuing the command: a failed write or flush may still have
        // delivered enough bytes for the terminal to enable bracketed paste.
        input.bracketed_paste = true;
        if let Err(error) = execute!(&mut *input.output, EnableBracketedPaste) {
            return Err(input.acquisition_failure(LauncherError::Interaction(format!(
                "enable bracketed paste for hidden input: {error}"
            ))));
        }
        if let Err(error) = input.output.flush() {
            return Err(input.acquisition_failure(LauncherError::Interaction(format!(
                "flush hidden-input terminal acquisition: {error}"
            ))));
        }
        Ok(input)
    }

    pub(crate) fn read_event(&mut self) -> Result<Event, LauncherError> {
        event::read().map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                input_ended("credential entry")
            } else {
                interaction(&error)
            }
        })
    }

    fn acquisition_failure(mut self, primary: LauncherError) -> LauncherError {
        let restoration = self.restore();
        merge_restoration(primary, &restoration)
    }

    fn finish<T>(mut self, result: Result<T, LauncherError>) -> Result<T, LauncherError> {
        let restoration = self.restore();
        match (result, restoration.is_empty()) {
            (result, true) => result,
            (Err(primary), false) => Err(merge_restoration(primary, &restoration)),
            (Ok(_), false) => Err(LauncherError::Interaction(format!(
                "hidden input completed, but terminal restoration failed: {}",
                restoration.join("; ")
            ))),
        }
    }

    fn restore(&mut self) -> Vec<String> {
        let mut diagnostics = Vec::new();
        if self.bracketed_paste {
            match execute!(&mut *self.output, DisableBracketedPaste) {
                Ok(()) => self.bracketed_paste = false,
                Err(error) => diagnostics.push(format!("disable bracketed paste: {error}")),
            }
        }
        if self.raw_mode {
            match disable_raw_mode() {
                Ok(()) => self.raw_mode = false,
                Err(error) => diagnostics.push(format!("disable raw mode: {error}")),
            }
        }
        diagnostics
    }
}

impl Drop for HiddenInput<'_> {
    fn drop(&mut self) {
        let _restoration_diagnostics = self.restore();
    }
}

pub(crate) fn input_cancelled(subject: &str) -> LauncherError {
    LauncherError::Interaction(format!(
        "{subject} cancelled by the user; run `peritus` to resume"
    ))
}

fn input_ended(subject: &str) -> LauncherError {
    LauncherError::Interaction(format!(
        "{subject} input ended; run `peritus` to resume"
    ))
}

fn merge_restoration(primary: LauncherError, restoration: &[String]) -> LauncherError {
    if restoration.is_empty() {
        primary
    } else {
        LauncherError::Interaction(format!(
            "{primary}; terminal restoration also failed: {}",
            restoration.join("; ")
        ))
    }
}

fn interaction(error: &io::Error) -> LauncherError {
    LauncherError::Interaction(error.to_string())
}
