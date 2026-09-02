//! Deterministic structural validation for changed CSV artifacts.

use std::{fs, path::Path};

use peritus_gates::GateExecutionRecord;

const MAX_CSV_BYTES: u64 = 64 * 1024 * 1024;

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    command: String,
) -> GateExecutionRecord {
    let csv_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_csv(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;

    for relative in &csv_paths {
        let path = workspace_root.join(relative);
        let result = validate_file(&path);
        match result {
            Ok(summary) => {
                output.push_str(&format!(
                    "{}: PASS ({} records, {} fields)",
                    relative.display(),
                    summary.records,
                    summary.fields,
                ));
                output.push('\n');
            }
            Err(detail) => {
                passed = false;
                output.push_str(&format!("{}: FAIL: {detail}\n", relative.display()));
            }
        }
    }

    if csv_paths.is_empty() {
        output.push_str("No changed CSV artifacts require structural validation.\n");
    }
    output.push_str(if passed { "CSV structure: PASS\n" } else { "CSV structure: FAIL\n" });

    GateExecutionRecord {
        command,
        label: "Artifact CSV structure".to_owned(),
        exit_code: Some(i32::from(!passed)),
        output,
    }
}

fn is_csv(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
}

fn validate_file(path: &Path) -> Result<CsvSummary, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("inspect file: {error}"))?;
    if metadata.len() > MAX_CSV_BYTES {
        return Err(format!("file exceeds the {MAX_CSV_BYTES}-byte validation limit"));
    }
    let bytes = fs::read(path).map_err(|error| format!("read file: {error}"))?;
    validate(&bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CsvSummary {
    records: usize,
    fields: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FieldState {
    Start,
    Unquoted,
    Quoted,
    AfterQuote,
}

fn validate(bytes: &[u8]) -> Result<CsvSummary, String> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if bytes.is_empty() {
        return Err("file is empty".to_owned());
    }
    std::str::from_utf8(bytes).map_err(|error| format!("file is not UTF-8: {error}"))?;
    CsvParser::new(bytes).parse()
}

struct CsvParser<'a> {
    bytes: &'a [u8],
    state: FieldState,
    index: usize,
    record: usize,
    fields: usize,
    expected_fields: Option<usize>,
    completed_records: usize,
    record_started: bool,
}

impl<'a> CsvParser<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            state: FieldState::Start,
            index: 0,
            record: 1,
            fields: 1,
            expected_fields: None,
            completed_records: 0,
            record_started: false,
        }
    }

    fn parse(mut self) -> Result<CsvSummary, String> {
        while self.index < self.bytes.len() {
            let byte = self.bytes[self.index];
            match self.state {
                FieldState::Start => self.consume_start(byte)?,
                FieldState::Unquoted => self.consume_unquoted(byte)?,
                FieldState::Quoted => self.consume_quoted(byte),
                FieldState::AfterQuote => self.consume_after_quote(byte)?,
            }
        }

        if self.state == FieldState::Quoted {
            return Err(format!("record {} has an unterminated quoted field", self.record));
        }
        if self.record_started {
            self.finish_record()?;
        }
        let fields = self.expected_fields.ok_or_else(|| "file has no records".to_owned())?;
        Ok(CsvSummary { records: self.completed_records, fields })
    }

    fn consume_start(&mut self, byte: u8) -> Result<(), String> {
        match byte {
            b',' => {
                self.fields += 1;
                self.record_started = true;
                self.index += 1;
            }
            b'"' => {
                self.state = FieldState::Quoted;
                self.record_started = true;
                self.index += 1;
            }
            b'\r' | b'\n' => self.finish_line(self.record_started)?,
            _ => {
                self.state = FieldState::Unquoted;
                self.record_started = true;
                self.index += 1;
            }
        }
        Ok(())
    }

    fn consume_unquoted(&mut self, byte: u8) -> Result<(), String> {
        match byte {
            b',' => {
                self.fields += 1;
                self.state = FieldState::Start;
                self.index += 1;
            }
            b'\r' | b'\n' => self.finish_line(true)?,
            b'"' => {
                return Err(format!(
                    "record {} contains a quote inside an unquoted field",
                    self.record
                ));
            }
            _ => self.index += 1,
        }
        Ok(())
    }

    fn consume_quoted(&mut self, byte: u8) {
        if byte == b'"' {
            if self.bytes.get(self.index + 1) == Some(&b'"') {
                self.index += 2;
            } else {
                self.state = FieldState::AfterQuote;
                self.index += 1;
            }
        } else {
            self.index += 1;
        }
    }

    fn consume_after_quote(&mut self, byte: u8) -> Result<(), String> {
        match byte {
            b',' => {
                self.fields += 1;
                self.state = FieldState::Start;
                self.index += 1;
            }
            b'\r' | b'\n' => self.finish_line(true)?,
            _ => {
                return Err(format!("record {} contains data after a closing quote", self.record));
            }
        }
        Ok(())
    }

    fn finish_line(&mut self, has_record: bool) -> Result<(), String> {
        if has_record {
            self.finish_record()?;
        }
        self.index = skip_record_end(self.bytes, self.index);
        self.record += 1;
        self.fields = 1;
        self.state = FieldState::Start;
        self.record_started = false;
        Ok(())
    }

    fn finish_record(&mut self) -> Result<(), String> {
        if let Some(expected) = self.expected_fields {
            if self.fields != expected {
                return Err(format!(
                    "record {} has {} fields; header has {expected}",
                    self.record, self.fields
                ));
            }
        } else {
            self.expected_fields = Some(self.fields);
        }
        self.completed_records += 1;
        Ok(())
    }
}

fn skip_record_end(bytes: &[u8], index: usize) -> usize {
    if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
        index + 2
    } else {
        index + 1
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn accepts_quoted_commas_quotes_and_newlines() {
        let csv = b"\r\nname,details\r\nalpha,plain\r\n\r\nbeta,\"comma, doubled \"\"quote\"\" and\nnewline\"\r\n";

        assert_eq!(validate(csv), Ok(CsvSummary { records: 3, fields: 2 }));
    }

    #[test]
    fn rejects_backslash_escaped_quotes_from_the_benchmark_regression() {
        let csv = br#"error_type,citation_key,details,expected_fix,evidence_span
doi_title_mismatch,Chen2024Duplicate,wrong DOI,replace it,"Bibliography: \"title\": \"Edge Operations Retrospective\", \"doi\": \"10.1000/wrong-doi\""
"#;

        let error = validate(csv).expect_err("backslash quote escaping must be rejected");

        assert!(error.contains("data after a closing quote"));
    }

    #[test]
    fn rejects_ragged_rows() {
        let error = validate(b"first,second\nvalue,extra,field\n").expect_err("ragged row");

        assert_eq!(error, "record 2 has 3 fields; header has 2");
    }

    #[test]
    fn gate_checks_only_changed_csv_artifacts() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir(root.path().join("out")).expect("output directory");
        fs::write(root.path().join("out/result.csv"), "a,b\n1,2,3\n").expect("CSV");
        fs::write(root.path().join("out/readme.md"), "artifact\n").expect("Markdown");

        let failed = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("out/result.csv"), PathBuf::from("out/readme.md")],
            "peritus-internal artifact-csv-structure".to_owned(),
        );
        let ignored = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("out/readme.md")],
            "peritus-internal artifact-csv-structure".to_owned(),
        );

        assert_eq!(failed.exit_code, Some(1));
        assert!(failed.output.contains("out/result.csv: FAIL"));
        assert_eq!(ignored.exit_code, Some(0));
        assert!(ignored.output.contains("No changed CSV artifacts"));
    }
}
