//! Small line-oriented terminal boundary for product setup screens.

use std::io::{self, BufRead, Write};

use crate::LauncherError;

pub struct Terminal<'a> {
    input: Box<dyn BufRead + 'a>,
    output: Box<dyn Write + 'a>,
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
            return Err(LauncherError::Interaction(
                "input ended before setup completed; run `peritus` to resume".to_owned(),
            ));
        }
        let answer = answer.trim().to_owned();
        if answer.eq_ignore_ascii_case("q") {
            return Err(LauncherError::Interaction(
                "setup cancelled; run `peritus` to resume".to_owned(),
            ));
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
}

fn interaction(error: &io::Error) -> LauncherError {
    LauncherError::Interaction(error.to_string())
}
