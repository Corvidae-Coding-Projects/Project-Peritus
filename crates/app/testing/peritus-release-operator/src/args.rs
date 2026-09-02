//! Closed command-line grammar for the release operator.

use std::{ffi::OsString, path::PathBuf};

use crate::error::OperatorError;

pub enum Operation {
    Generate { record: PathBuf },
    Publish { record: PathBuf, provenance_bundle: PathBuf, sbom_bundle: PathBuf },
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Operation, OperatorError> {
    let mut args = args.into_iter();
    let command = text(args.next(), "a command")?;
    match command.as_str() {
        "generate" => {
            let record = path(args.next(), "native package record")?;
            reject_extra(args)?;
            Ok(Operation::Generate { record })
        }
        "retain-and-upload" => {
            let record = path(args.next(), "native package record")?;
            let provenance_bundle = path(args.next(), "build provenance bundle")?;
            let sbom_bundle = path(args.next(), "SBOM attestation bundle")?;
            reject_extra(args)?;
            Ok(Operation::Publish { record, provenance_bundle, sbom_bundle })
        }
        _ => Err(OperatorError::usage()),
    }
}

fn text(value: Option<OsString>, name: &'static str) -> Result<String, OperatorError> {
    value
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| OperatorError::argument(format!("expected {name}")))
}

fn path(value: Option<OsString>, name: &'static str) -> Result<PathBuf, OperatorError> {
    value.map(PathBuf::from).ok_or_else(|| OperatorError::argument(format!("expected {name}")))
}

fn reject_extra(mut args: impl Iterator<Item = OsString>) -> Result<(), OperatorError> {
    if args.next().is_some() { Err(OperatorError::usage()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::{Operation, parse};
    use std::ffi::OsString;

    #[test]
    fn generate_requires_one_record() {
        assert!(matches!(
            parse([OsString::from("generate"), OsString::from("record.json")])
                .expect("valid command"),
            Operation::Generate { .. }
        ));
        assert!(parse([OsString::from("generate")]).is_err());
    }

    #[test]
    fn publication_requires_both_attestation_bundles() {
        let args = ["retain-and-upload", "record.json", "provenance.jsonl", "sbom.jsonl"]
            .map(OsString::from);
        assert!(matches!(parse(args).expect("valid command"), Operation::Publish { .. }));
    }
}
