//! Contract-selected, incremental structural validation for changed CSV artifacts.

use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use peritus_gates::{GateExecutionRecord, GateObservation};

use super::{GateOutcome, cancellation::GateCancellation};
mod parser;
use parser::{CsvParser, CsvSummary, Encoding};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CsvContract {
    delimiter: char,
    encoding: Encoding,
    rectangular: bool,
}

enum ContractSelection {
    Absent,
    Incomplete,
    Complete(CsvContract),
}

pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[PathBuf],
    command: String,
    request_context: &str,
    cancellation: &GateCancellation,
) -> GateOutcome {
    let csv_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_csv(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;
    let mut unevaluated = false;
    let mut required = false;

    if csv_paths.is_empty() {
        return GateOutcome::Optional(GateObservation {
            command,
            label: "Artifact CSV structure".to_owned(),
            output:
                "NOT APPLICABLE: no changed CSV artifact selected a structural acceptance check"
                    .to_owned(),
        });
    }

    for relative in &csv_paths {
        let contract = match contract_for(workspace_root, relative, request_context) {
            Ok(ContractSelection::Complete(contract)) => {
                required = true;
                contract
            }
            Ok(ContractSelection::Absent) => {
                let _ = writeln!(
                    output,
                    "{}: NOT EVALUATED (no CSV structural acceptance check was selected for this file)",
                    relative.display(),
                );
                continue;
            }
            Ok(ContractSelection::Incomplete) => {
                required = true;
                let _ = writeln!(
                    output,
                    "{}: NOT EVALUATED (the selected CSV contract does not declare delimiter, encoding, and row shape)",
                    relative.display(),
                );
                unevaluated = true;
                continue;
            }
            Err(detail) => {
                required = true;
                passed = false;
                let _ = writeln!(
                    output,
                    "{}: FAIL: invalid CSV contract: {detail}",
                    relative.display(),
                );
                continue;
            }
        };
        if cancellation.is_cancelled() {
            let _ = writeln!(output, "{}: NOT EVALUATED (run was cancelled)", relative.display());
            unevaluated = true;
            break;
        }
        match validate_file(&workspace_root.join(relative), contract, cancellation) {
            Ok(summary) => {
                let _ = writeln!(
                    output,
                    "{}: PASS ({} records, {} fields in first record; delimiter {:?}, {:?})",
                    relative.display(),
                    summary.records,
                    summary.fields,
                    contract.delimiter,
                    contract.encoding,
                );
            }
            Err(detail) => {
                if cancellation.is_cancelled() {
                    let _ = writeln!(
                        output,
                        "{}: NOT EVALUATED (run was cancelled while reading or parsing)",
                        relative.display(),
                    );
                    unevaluated = true;
                    break;
                }
                passed = false;
                let _ = writeln!(output, "{}: FAIL: {detail}", relative.display());
            }
        }
    }

    outcome(command, output, required, passed, unevaluated)
}

fn outcome(
    command: String,
    mut output: String,
    required: bool,
    passed: bool,
    unevaluated: bool,
) -> GateOutcome {
    if !required {
        return GateOutcome::Optional(GateObservation {
            command,
            label: "Artifact CSV structure".to_owned(),
            output: format!(
                "NOT EVALUATED: no CSV structural acceptance check was selected\n{output}"
            ),
        });
    }
    output.push_str(if !passed {
        "CSV structure: FAIL\n"
    } else if unevaluated {
        "CSV structure: NOT EVALUATED\n"
    } else {
        "CSV structure: PASS\n"
    });
    GateOutcome::Required(GateExecutionRecord {
        command,
        label: "Artifact CSV structure".to_owned(),
        exit_code: if !passed {
            Some(1)
        } else if unevaluated {
            None
        } else {
            Some(0)
        },
        output,
    })
}

fn is_csv(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
}

fn contract_for(
    workspace_root: &Path,
    path: &Path,
    request_context: &str,
) -> Result<ContractSelection, String> {
    let mut active = false;
    let mut found = false;
    let mut delimiter = None;
    let mut encoding = None;
    let mut rectangular = None;
    for line in request_context.lines() {
        let line = line.trim().trim_start_matches(['-', '*']).trim();
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("csv contract for ") {
            let declared = &line["CSV contract for ".len()..];
            active = declared
                .strip_suffix(':')
                .map(|declared| unquote(declared).unwrap_or(declared))
                .and_then(|declared| resolve_contract_path(workspace_root, declared))
                .is_some_and(|declared| declared == path);
            if active {
                if found {
                    return Err("more than one contract names this file".to_owned());
                }
                found = true;
            }
            continue;
        }
        if !active {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = unquote(value.trim()).unwrap_or_else(|| value.trim());
        match key.trim().to_ascii_lowercase().as_str() {
            "delimiter" => delimiter = Some(parse_delimiter(value)?),
            "encoding" => encoding = Some(parse_encoding(value)?),
            "row shape" | "rows" => {
                rectangular = Some(parse_row_shape(value)?);
            }
            _ => {}
        }
    }
    if !found {
        return Ok(ContractSelection::Absent);
    }
    match (delimiter, encoding, rectangular) {
        (Some(delimiter), Some(encoding), Some(rectangular)) => {
            Ok(ContractSelection::Complete(CsvContract { delimiter, encoding, rectangular }))
        }
        _ => Ok(ContractSelection::Incomplete),
    }
}

fn resolve_contract_path(root: &Path, declared: &str) -> Option<PathBuf> {
    let declared = PathBuf::from(declared);
    if declared.is_absolute() {
        return declared.strip_prefix(root).ok().map(Path::to_path_buf);
    }
    let mut normalized = PathBuf::new();
    for component in declared.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::RootDir
            | std::path::Component::ParentDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    Some(normalized)
}

fn unquote(value: &str) -> Option<&str> {
    let value = value.trim();
    let first = value.chars().next()?;
    let last = value.chars().next_back()?;
    (value.len() >= 2 && matches!(first, '`' | '\'' | '"') && first == last)
        .then(|| &value[first.len_utf8()..value.len() - last.len_utf8()])
}

fn parse_delimiter(value: &str) -> Result<char, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "comma" | "," => Ok(','),
        "semicolon" | ";" => Ok(';'),
        "tab" | "\\t" => Ok('\t'),
        "pipe" | "|" => Ok('|'),
        _ => {
            let mut chars = value.chars();
            let delimiter = chars.next().ok_or_else(|| "delimiter is empty".to_owned())?;
            if chars.next().is_some() || matches!(delimiter, '\r' | '\n' | '"') {
                Err("delimiter must be one non-quote character".to_owned())
            } else {
                Ok(delimiter)
            }
        }
    }
}

fn parse_encoding(value: &str) -> Result<Encoding, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "utf-8" | "utf8" => Ok(Encoding::Utf8),
        "latin-1" | "latin1" | "iso-8859-1" => Ok(Encoding::Latin1),
        "utf-16le" => Ok(Encoding::Utf16Le),
        "utf-16be" => Ok(Encoding::Utf16Be),
        other => Err(format!("unsupported declared encoding {other:?}")),
    }
}

fn parse_row_shape(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "rectangular" | "equal-width" | "same-width" => Ok(true),
        "ragged" | "unrestricted" | "any" => Ok(false),
        other => Err(format!("unsupported declared row shape {other:?}")),
    }
}

fn validate_file(
    path: &Path,
    contract: CsvContract,
    cancellation: &GateCancellation,
) -> Result<CsvSummary, String> {
    let file = fs::File::open(path).map_err(|error| format!("read file: {error}"))?;
    CsvParser::new(file, contract, cancellation)?.parse()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn parse(bytes: &[u8], delimiter: char, rectangular: bool) -> Result<CsvSummary, String> {
        let cancellation = GateCancellation::default();
        CsvParser::new(
            Cursor::new(bytes.to_vec()),
            CsvContract { delimiter, encoding: Encoding::Utf8, rectangular },
            &cancellation,
        )?
        .parse()
    }

    #[test]
    fn parses_quoted_delimiters_and_multiline_fields_incrementally() {
        let csv = b"\r\nname;details\r\nalpha;plain\r\n\r\nbeta;\"semi; doubled \"\"quote\"\" and\nnewline\"\r\n";
        assert_eq!(parse(csv, ';', true), Ok(CsvSummary { records: 3, fields: 2 }));
    }

    #[test]
    fn row_shape_is_checked_only_when_selected_by_the_contract() {
        assert!(parse(b"a,b\n1,2,3\n", ',', true).is_err());
        assert_eq!(parse(b"a,b\n1,2,3\n", ',', false), Ok(CsvSummary { records: 2, fields: 2 }));
    }

    #[test]
    fn a_missing_file_contract_is_not_treated_as_pass_evidence() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("result.csv"), "a;b\n1;2\n").expect("CSV");
        let record = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("result.csv")],
            "artifact-csv-structure".to_owned(),
            "",
            &GateCancellation::default(),
        );
        let GateOutcome::Optional(observation) = record else {
            panic!("missing CSV contract must be optional");
        };
        assert!(observation.output.contains("NOT EVALUATED"));
    }
}
