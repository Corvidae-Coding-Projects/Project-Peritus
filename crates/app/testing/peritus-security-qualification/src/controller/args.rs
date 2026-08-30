//! Exact native-controller process arguments.

use std::ffi::OsString;
use std::path::PathBuf;

use super::error::ControllerError;

pub(super) struct Options {
    pub(super) request: PathBuf,
    pub(super) response: PathBuf,
    pub(super) subject_root: PathBuf,
    pub(super) artifact_root: PathBuf,
    pub(super) subject_id: String,
    pub(super) request_sha256: String,
    pub(super) candidate_root: PathBuf,
}

impl Options {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, ControllerError> {
        if arguments.len() != 14 {
            return Err(ControllerError::Arguments(usage()));
        }
        let mut request = None;
        let mut response = None;
        let mut subject_root = None;
        let mut artifact_root = None;
        let mut subject_id = None;
        let mut request_sha256 = None;
        let mut candidate_root = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or(ControllerError::Arguments(usage()))?;
            match name {
                "--request" => set_once(&mut request, PathBuf::from(&pair[1]))?,
                "--response" => set_once(&mut response, PathBuf::from(&pair[1]))?,
                "--subject-root" => set_once(&mut subject_root, PathBuf::from(&pair[1]))?,
                "--artifact-root" => set_once(&mut artifact_root, PathBuf::from(&pair[1]))?,
                "--subject-id" => set_once(
                    &mut subject_id,
                    pair[1].to_str().ok_or(ControllerError::Arguments(usage()))?.to_owned(),
                )?,
                "--request-sha256" => set_once(
                    &mut request_sha256,
                    pair[1].to_str().ok_or(ControllerError::Arguments(usage()))?.to_owned(),
                )?,
                "--candidate-root" => set_once(&mut candidate_root, PathBuf::from(&pair[1]))?,
                _ => return Err(ControllerError::Arguments(usage())),
            }
        }
        Ok(Self {
            request: required(request)?,
            response: required(response)?,
            subject_root: required(subject_root)?,
            artifact_root: required(artifact_root)?,
            subject_id: required(subject_id)?,
            request_sha256: required(request_sha256)?,
            candidate_root: required(candidate_root)?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), ControllerError> {
    if slot.replace(value).is_some() {
        return Err(ControllerError::Arguments(usage()));
    }
    Ok(())
}

fn required<T>(value: Option<T>) -> Result<T, ControllerError> {
    value.ok_or(ControllerError::Arguments(usage()))
}

const fn usage() -> &'static str {
    "usage: peritus-h0-controller --request PATH --response PATH --subject-root DIR --artifact-root DIR --subject-id ID --request-sha256 HEX --candidate-root DIR"
}
