//! Deterministic syntax validation for changed YAML configuration files.

use std::{
    fs,
    io::{self, BufRead as _, Read as _},
    path::Path,
};

use peritus_gates::GateExecutionRecord;
use yaml_rust2::parser::{Event, EventReceiver, Parser};

use super::cancellation::{CancellableReader, GateCancellation};

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    command: String,
    cancellation: &GateCancellation,
) -> GateExecutionRecord {
    let yaml_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_yaml(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;
    let mut unevaluated = false;

    for relative in &yaml_paths {
        if cancellation.is_cancelled() {
            unevaluated = true;
            output.push_str("YAML structure: NOT EVALUATED (run was cancelled)\n");
            break;
        }
        match validate_file(&workspace_root.join(relative), cancellation) {
            Ok(document_count) => {
                output.push_str(&format!(
                    "{}: PASS ({document_count} document(s))\n",
                    relative.display(),
                ));
            }
            Err(detail) => {
                if cancellation.is_cancelled() {
                    unevaluated = true;
                    output.push_str(&format!(
                        "{}: NOT EVALUATED (run was cancelled while reading or parsing)\n",
                        relative.display(),
                    ));
                    break;
                }
                passed = false;
                output.push_str(&format!("{}: FAIL: {detail}\n", relative.display()));
            }
        }
    }

    if yaml_paths.is_empty() {
        output.push_str("No changed YAML files require structural validation.\n");
    }
    output.push_str(if !passed {
        "YAML structure: FAIL\n"
    } else if unevaluated {
        "YAML structure: NOT EVALUATED\n"
    } else {
        "YAML structure: PASS\n"
    });

    GateExecutionRecord {
        command,
        label: "YAML structure".to_owned(),
        exit_code: if !passed {
            Some(1)
        } else if unevaluated {
            None
        } else {
            Some(0)
        },
        output,
    }
}

fn is_yaml(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yml" | "yaml"))
}

fn validate_file(path: &Path, cancellation: &GateCancellation) -> Result<usize, String> {
    let file = fs::File::open(path).map_err(|error| format!("read file: {error}"))?;
    let mut source = YamlChars::new(file, cancellation).map_err(|error| error.to_string())?;
    let mut document_counter = DocumentCounter::default();
    let parse_result = {
        let mut parser = Parser::new(&mut source);
        parser.load(&mut document_counter, true)
    };
    if let Some(error) = source.error {
        return Err(format!("read file: {error}"));
    }
    parse_result.map_err(|error| error.to_string())?;
    if document_counter.overflowed {
        return Err("YAML document count overflow".to_owned());
    }
    Ok(document_counter.documents)
}

#[derive(Default)]
struct DocumentCounter {
    documents: usize,
    overflowed: bool,
}

impl EventReceiver for DocumentCounter {
    fn on_event(&mut self, event: Event) {
        if event == Event::DocumentStart {
            match self.documents.checked_add(1) {
                Some(documents) => self.documents = documents,
                None => self.overflowed = true,
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

struct YamlChars<R> {
    reader: io::BufReader<CancellableReader<R>>,
    encoding: Encoding,
    error: Option<io::Error>,
}

impl<R: io::Read> YamlChars<R> {
    fn new(inner: R, cancellation: &GateCancellation) -> io::Result<Self> {
        let mut reader = io::BufReader::new(cancellation.reader(inner));
        let available = reader.fill_buf()?;
        let (encoding, bom) = if available.starts_with(&[0, 0, 0xfe, 0xff]) {
            (Encoding::Utf32Be, 4)
        } else if available.starts_with(&[0xff, 0xfe, 0, 0]) {
            (Encoding::Utf32Le, 4)
        } else if available.starts_with(&[0xfe, 0xff]) {
            (Encoding::Utf16Be, 2)
        } else if available.starts_with(&[0xff, 0xfe]) {
            (Encoding::Utf16Le, 2)
        } else if available.starts_with(&[0xef, 0xbb, 0xbf]) {
            (Encoding::Utf8, 3)
        } else {
            (Encoding::Utf8, 0)
        };
        reader.consume(bom);
        Ok(Self { reader, encoding, error: None })
    }

    fn read_byte(&mut self) -> io::Result<Option<u8>> {
        let mut byte = [0];
        match self.reader.read(&mut byte)? {
            0 => Ok(None),
            _ => Ok(Some(byte[0])),
        }
    }

    fn read_unit(&mut self, first: u8, width: usize) -> io::Result<u32> {
        let mut bytes = [0; 4];
        bytes[0] = first;
        self.reader.read_exact(&mut bytes[1..width])?;
        Ok(match (self.encoding, width) {
            (Encoding::Utf16Le, 2) => u32::from(u16::from_le_bytes([bytes[0], bytes[1]])),
            (Encoding::Utf16Be, 2) => u32::from(u16::from_be_bytes([bytes[0], bytes[1]])),
            (Encoding::Utf32Le, 4) => u32::from_le_bytes(bytes),
            (Encoding::Utf32Be, 4) => u32::from_be_bytes(bytes),
            _ => unreachable!("encoding and unit width are selected together"),
        })
    }

    fn next_character(&mut self) -> io::Result<Option<char>> {
        let Some(first) = self.read_byte()? else { return Ok(None) };
        let scalar = match self.encoding {
            Encoding::Utf8 => {
                let width = match first {
                    0x00..=0x7f => 1,
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-8 lead byte",
                        ));
                    }
                };
                let mut bytes = [0; 4];
                bytes[0] = first;
                self.reader.read_exact(&mut bytes[1..width])?;
                let text = std::str::from_utf8(&bytes[..width])
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                return Ok(text.chars().next());
            }
            Encoding::Utf16Le | Encoding::Utf16Be => {
                let unit = u16::try_from(self.read_unit(first, 2)?)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                if (0xd800..=0xdbff).contains(&unit) {
                    let low_first = self.read_byte()?.ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "truncated UTF-16 surrogate pair",
                        )
                    })?;
                    let low = u16::try_from(self.read_unit(low_first, 2)?)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid UTF-16 surrogate pair",
                        ));
                    }
                    0x10000 + (((u32::from(unit) - 0xd800) << 10) | (u32::from(low) - 0xdc00))
                } else {
                    u32::from(unit)
                }
            }
            Encoding::Utf32Le | Encoding::Utf32Be => self.read_unit(first, 4)?,
        };
        char::from_u32(scalar).map(Some).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid encoded YAML character")
        })
    }
}

impl<R: io::Read> Iterator for YamlChars<R> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.error.is_some() {
            return None;
        }
        match self.next_character() {
            Ok(character) => character,
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn validates_only_changed_yaml_files_in_the_project() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(root.path().join("project/.github/workflows")).expect("directory");
        fs::write(
            root.path().join("project/.github/workflows/ci.yml"),
            "name: CI\non: [push]\njobs: {}\n",
        )
        .expect("workflow");
        fs::write(root.path().join("outside.yaml"), "broken: [\n").expect("outside YAML");

        let record = run(
            root.path(),
            Path::new("project"),
            &[PathBuf::from("project/.github/workflows/ci.yml"), PathBuf::from("outside.yaml")],
            "yaml-structure".to_owned(),
            &GateCancellation::default(),
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("project/.github/workflows/ci.yml: PASS"));
        assert!(!record.output.contains("outside.yaml"));
    }

    #[test]
    fn rejects_malformed_yaml() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("broken.yml"), "jobs: [\n").expect("malformed YAML");

        let record = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("broken.yml")],
            "yaml-structure".to_owned(),
            &GateCancellation::default(),
        );

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("broken.yml: FAIL"));
    }
}
