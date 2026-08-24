//! Complete bounded quality output parsing.

use peritus_process::OutputCompleteness;

use crate::{OutputParser, QualityError, QualityErrorKind};

/// Result of applying a frozen output parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedOutput {
    NotRequested,
    Utf8,
    Json(serde_json::Value),
}

pub fn parse(
    parser: OutputParser,
    output: &[u8],
    completeness: OutputCompleteness,
    sequence_complete: bool,
) -> Result<ParsedOutput, QualityError> {
    if parser == OutputParser::None {
        return Ok(ParsedOutput::NotRequested);
    }
    if completeness != OutputCompleteness::Complete || !sequence_complete {
        return Err(QualityError::new(
            QualityErrorKind::Parser,
            "complete ordered process output is unavailable",
        ));
    }
    let maximum = parser.maximum_bytes().expect("non-None parser has a byte bound");
    if output.len() > maximum as usize {
        return Err(QualityError::new(
            QualityErrorKind::Parser,
            "process output exceeds the selected parser bound",
        ));
    }
    let text = std::str::from_utf8(output).map_err(|_| {
        QualityError::new(QualityErrorKind::Parser, "process output is not valid UTF-8")
    })?;
    match parser {
        OutputParser::None => Ok(ParsedOutput::NotRequested),
        OutputParser::Utf8 { .. } => Ok(ParsedOutput::Utf8),
        OutputParser::Json { .. } => {
            serde_json::from_str(text).map(ParsedOutput::Json).map_err(|error| {
                QualityError::new(
                    QualityErrorKind::Parser,
                    format!("process output is not one valid JSON value: {error}"),
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parser_never_passes_invalid_or_incomplete_output() {
        let parser = OutputParser::Json { maximum_bytes: 64 };
        assert!(parse(parser, b"not-json", OutputCompleteness::Complete, true).is_err());
        assert!(parse(parser, b"{}", OutputCompleteness::Incomplete, true).is_err());
        assert!(parse(parser, b"{}", OutputCompleteness::Complete, false).is_err());
        assert!(matches!(
            parse(parser, b"{\"ok\":true}", OutputCompleteness::Complete, true),
            Ok(ParsedOutput::Json(_))
        ));
    }
}
