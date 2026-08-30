//! Closed command-line grammar for the H4 evidence operator.

use std::{ffi::OsString, path::PathBuf};

use crate::{EvidenceDisposition, EvidenceKind};

use super::OperatorError;

pub(super) enum Operation {
    Envelope(EvidenceInput),
    Verify(VerificationInput),
    Finalize(FinalizeInput),
}

pub(super) struct EvidenceInput {
    pub binding: PathBuf,
    pub kind: EvidenceKind,
    pub disposition: EvidenceDisposition,
    pub retained_path: String,
    pub payload: PathBuf,
    pub output: PathBuf,
}

pub(super) struct VerificationInput {
    pub evidence: EvidenceInput,
    pub key_id: String,
    pub public_key: PathBuf,
    pub signature: PathBuf,
}

pub(super) struct FinalizeInput {
    pub plan: PathBuf,
    pub evidence_root: PathBuf,
    pub output: PathBuf,
}

pub(super) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Operation, OperatorError> {
    let mut args = args.into_iter();
    let command = text(args.next(), "command")?;
    let mut input = RawInput::default();
    while let Some(flag) = args.next() {
        let flag = flag.into_string().map_err(|_| OperatorError::usage())?;
        let value = args.next().ok_or_else(OperatorError::usage)?;
        input.set(&flag, value)?;
    }
    match command.as_str() {
        "envelope" => Ok(Operation::Envelope(input.evidence()?)),
        "verify" => Ok(Operation::Verify(input.verification()?)),
        "finalize" => Ok(Operation::Finalize(input.finalize()?)),
        _ => Err(OperatorError::usage()),
    }
}

#[derive(Default)]
struct RawInput {
    binding: Option<PathBuf>,
    kind: Option<EvidenceKind>,
    disposition: Option<EvidenceDisposition>,
    retained_path: Option<String>,
    payload: Option<PathBuf>,
    output: Option<PathBuf>,
    key_id: Option<String>,
    public_key: Option<PathBuf>,
    signature: Option<PathBuf>,
    plan: Option<PathBuf>,
    evidence_root: Option<PathBuf>,
}

impl RawInput {
    fn set(&mut self, flag: &str, value: OsString) -> Result<(), OperatorError> {
        match flag {
            "--binding" => set_once(&mut self.binding, PathBuf::from(value), flag),
            "--kind" => set_once(&mut self.kind, parse_json_enum(value, flag)?, flag),
            "--disposition" => set_once(&mut self.disposition, parse_json_enum(value, flag)?, flag),
            "--retained-path" => set_once(&mut self.retained_path, text(Some(value), flag)?, flag),
            "--payload" => set_once(&mut self.payload, PathBuf::from(value), flag),
            "--output" => set_once(&mut self.output, PathBuf::from(value), flag),
            "--key-id" => set_once(&mut self.key_id, text(Some(value), flag)?, flag),
            "--public-key" => set_once(&mut self.public_key, PathBuf::from(value), flag),
            "--signature" => set_once(&mut self.signature, PathBuf::from(value), flag),
            "--plan" => set_once(&mut self.plan, PathBuf::from(value), flag),
            "--evidence-root" => set_once(&mut self.evidence_root, PathBuf::from(value), flag),
            _ => Err(OperatorError::argument(format!("unknown option {flag}"))),
        }
    }

    fn evidence(&self) -> Result<EvidenceInput, OperatorError> {
        Ok(EvidenceInput {
            binding: required(self.binding.as_ref(), "--binding")?,
            kind: required(self.kind.as_ref(), "--kind")?,
            disposition: required(self.disposition.as_ref(), "--disposition")?,
            retained_path: required(self.retained_path.as_ref(), "--retained-path")?,
            payload: required(self.payload.as_ref(), "--payload")?,
            output: required(self.output.as_ref(), "--output")?,
        })
    }

    fn verification(&self) -> Result<VerificationInput, OperatorError> {
        Ok(VerificationInput {
            evidence: self.evidence()?,
            key_id: required(self.key_id.as_ref(), "--key-id")?,
            public_key: required(self.public_key.as_ref(), "--public-key")?,
            signature: required(self.signature.as_ref(), "--signature")?,
        })
    }

    fn finalize(&self) -> Result<FinalizeInput, OperatorError> {
        Ok(FinalizeInput {
            plan: required(self.plan.as_ref(), "--plan")?,
            evidence_root: required(self.evidence_root.as_ref(), "--evidence-root")?,
            output: required(self.output.as_ref(), "--output")?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), OperatorError> {
    if slot.replace(value).is_some() {
        Err(OperatorError::argument(format!("duplicate option {flag}")))
    } else {
        Ok(())
    }
}

fn required<T: Clone>(value: Option<&T>, flag: &str) -> Result<T, OperatorError> {
    value.cloned().ok_or_else(|| OperatorError::argument(format!("missing {flag}")))
}

fn parse_json_enum<T: serde::de::DeserializeOwned>(
    value: OsString,
    flag: &str,
) -> Result<T, OperatorError> {
    let value = text(Some(value), flag)?;
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| OperatorError::argument(format!("invalid value for {flag}: {value}")))
}

fn text(value: Option<OsString>, name: &str) -> Result<String, OperatorError> {
    value
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| OperatorError::argument(format!("{name} must be UTF-8")))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Operation, parse};

    #[test]
    fn envelope_requires_the_complete_closed_argument_set() {
        let args = [
            "envelope",
            "--binding",
            "binding.json",
            "--kind",
            "h0-security-report",
            "--disposition",
            "satisfied",
            "--retained-path",
            "reports/h0.json",
            "--payload",
            "h0.json",
            "--output",
            "h0.envelope.json",
        ]
        .map(OsString::from);
        assert!(matches!(parse(args).expect("valid arguments"), Operation::Envelope(_)));
    }

    #[test]
    fn verify_requires_public_material() {
        let args = [
            "verify",
            "--binding",
            "binding.json",
            "--kind",
            "h0-security-report",
            "--disposition",
            "satisfied",
            "--retained-path",
            "reports/h0.json",
            "--payload",
            "h0.json",
            "--output",
            "h0.record.json",
        ]
        .map(OsString::from);
        assert!(parse(args).is_err());
    }
}
