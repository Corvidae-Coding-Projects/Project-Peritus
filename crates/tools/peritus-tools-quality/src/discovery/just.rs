//! Incremental zero-argument public Just recipe discovery.

use std::{
    collections::BTreeSet,
    io::{self, Write},
};

use crate::{CheckDefinition, CheckSource};

use super::{
    DiscoveryCause, DiscoveryCoverage, DiscoveryDiagnostic, discovered_definition,
};

pub(super) fn discover(
    filename: &str,
    bytes: &[u8],
    definitions: &mut Vec<CheckDefinition>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let mut discovery = StreamingDiscovery::new(filename);
    discovery.write_all(bytes).expect("in-memory Just discovery writer cannot fail");
    let (discovered, observed) = discovery.finish();
    definitions.extend(discovered);
    diagnostics.extend(observed);
}

/// Source owner which parses every physical workspace-copy chunk without a logical file, line, or
/// recipe allowance. Only the current incomplete line and canonical recipe names are retained.
pub(super) struct StreamingDiscovery {
    filename: String,
    pending_line: Vec<u8>,
    line_number: usize,
    names: BTreeSet<String>,
    unsupported: usize,
    first_unsupported: Option<usize>,
    duplicates: usize,
    invalid_utf8: bool,
}

impl StreamingDiscovery {
    pub(super) fn new(filename: &str) -> Self {
        Self {
            filename: filename.to_owned(),
            pending_line: Vec::new(),
            line_number: 0,
            names: BTreeSet::new(),
            unsupported: 0,
            first_unsupported: None,
            duplicates: 0,
            invalid_utf8: false,
        }
    }

    pub(super) fn finish(mut self) -> (Vec<CheckDefinition>, Vec<DiscoveryDiagnostic>) {
        if !self.pending_line.is_empty() {
            let line = std::mem::take(&mut self.pending_line);
            self.accept_line(&line);
        }
        if self.invalid_utf8 {
            return (
                Vec::new(),
                vec![DiscoveryDiagnostic::new(
                    self.filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Unavailable,
                    "Justfile is not valid UTF-8",
                )],
            );
        }

        let mut diagnostics = Vec::new();
        if self.unsupported > 0 {
            diagnostics.push(DiscoveryDiagnostic::new(
                &self.filename,
                DiscoveryCause::UnsupportedSyntax,
                DiscoveryCoverage::Partial,
                format!(
                    "{} top-level declaration(s) use syntax outside the incremental public zero-argument recipe grammar; first occurrence is line {}",
                    self.unsupported,
                    self.first_unsupported
                        .expect("positive unsupported count has a first line")
                ),
            ));
        }
        if self.duplicates > 0 {
            diagnostics.push(DiscoveryDiagnostic::new(
                &self.filename,
                DiscoveryCause::InvalidSyntax,
                DiscoveryCoverage::Partial,
                format!(
                    "{} duplicate public recipe declaration(s) were reconciled to one stable gate name",
                    self.duplicates
                ),
            ));
        }
        let mut definitions = Vec::with_capacity(self.names.len());
        for name in self.names {
            match discovered_definition(
                &format!("just.{name}"),
                CheckSource::JustfileRecipe(format!("{}:{name}", self.filename)),
                "just",
                vec![name],
            ) {
                Ok(definition) => definitions.push(definition),
                Err(error) => diagnostics.push(DiscoveryDiagnostic::new(
                    &self.filename,
                    DiscoveryCause::UnsupportedSyntax,
                    DiscoveryCoverage::Partial,
                    error.detail(),
                )),
            }
        }
        (definitions, diagnostics)
    }

    fn accept_line(&mut self, bytes: &[u8]) {
        self.line_number = self.line_number.saturating_add(1);
        let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        let Ok(line) = std::str::from_utf8(bytes) else {
            self.invalid_utf8 = true;
            return;
        };
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.len() != line.len()
        {
            return;
        }
        match classify_top_level(trimmed) {
            TopLevel::Recipe(name) => {
                if !self.names.insert(name.to_owned()) {
                    self.duplicates = self.duplicates.saturating_add(1);
                }
            }
            TopLevel::Skip => {}
            TopLevel::Unsupported => {
                self.unsupported = self.unsupported.saturating_add(1);
                self.first_unsupported.get_or_insert(self.line_number);
            }
        }
    }
}

impl Write for StreamingDiscovery {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.pending_line.extend_from_slice(bytes);
        let mut consumed = 0;
        while let Some(relative) = self.pending_line[consumed..].iter().position(|byte| *byte == b'\n')
        {
            let end = consumed + relative;
            let line = self.pending_line[consumed..end].to_vec();
            self.accept_line(&line);
            consumed = end + 1;
        }
        if consumed > 0 {
            self.pending_line.drain(..consumed);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

enum TopLevel<'a> {
    Recipe(&'a str),
    Skip,
    Unsupported,
}

fn classify_top_level(line: &str) -> TopLevel<'_> {
    if known_non_recipe(line) {
        return TopLevel::Skip;
    }
    let declaration = line.strip_prefix('@').unwrap_or(line);
    let Some((head, tail)) = declaration.split_once(':') else {
        return TopLevel::Unsupported;
    };
    if tail.starts_with('=')
        || head.is_empty()
        || head.contains(char::is_whitespace)
        || !head.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return TopLevel::Unsupported;
    }
    if head.starts_with('_') {
        TopLevel::Skip
    } else {
        TopLevel::Recipe(head)
    }
}

fn known_non_recipe(line: &str) -> bool {
    if line.starts_with("set ") || line.starts_with("unexport ") {
        return true;
    }
    let assignment = line.strip_prefix("export ").unwrap_or(line);
    [":=", "?=", "+=", "="].into_iter().any(|operator| {
        assignment.split_once(operator).is_some_and(|(name, _)| {
            !name.is_empty()
                && !name.contains(char::is_whitespace)
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_only_public_zero_argument_recipes() {
        let mut definitions = Vec::new();
        let mut diagnostics = Vec::new();
        discover(
            "Justfile",
            b"check:\n  cargo check\nwith-arg value:\n  echo {{value}}\n_private:\n  true\n",
            &mut definitions,
            &mut diagnostics,
        );
        let names: Vec<_> = definitions.iter().map(CheckDefinition::gate_name).collect();
        assert_eq!(names, ["just.check"]);
        assert_eq!(diagnostics[0].cause(), DiscoveryCause::UnsupportedSyntax);
    }
}
