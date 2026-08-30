//! Exact adapter-owned controller invocation.

use std::ffi::OsString;
use std::path::PathBuf;

pub(super) struct ControllerPaths {
    pub(super) request: PathBuf,
    pub(super) response: PathBuf,
    pub(super) cleanup_response: PathBuf,
    pub(super) subject_root: PathBuf,
    pub(super) package_root: PathBuf,
    pub(super) artifact_root: PathBuf,
    pub(super) subject_id: String,
    pub(super) request_sha256: String,
}

impl ControllerPaths {
    pub(super) fn parse(arguments: &[OsString]) -> Result<Self, &'static str> {
        if arguments.len() != 16 {
            return Err(usage());
        }
        let mut request = None;
        let mut response = None;
        let mut cleanup_response = None;
        let mut subject_root = None;
        let mut package_root = None;
        let mut artifact_root = None;
        let mut subject_id = None;
        let mut request_sha256 = None;
        for pair in arguments.chunks_exact(2) {
            let name = pair[0].to_str().ok_or_else(usage)?;
            match name {
                "--request" => set_once(&mut request, PathBuf::from(&pair[1]))?,
                "--response" => set_once(&mut response, PathBuf::from(&pair[1]))?,
                "--cleanup-response" => {
                    set_once(&mut cleanup_response, PathBuf::from(&pair[1]))?;
                }
                "--subject-root" => set_once(&mut subject_root, PathBuf::from(&pair[1]))?,
                "--package-root" => set_once(&mut package_root, PathBuf::from(&pair[1]))?,
                "--artifact-root" => set_once(&mut artifact_root, PathBuf::from(&pair[1]))?,
                "--subject-id" => {
                    set_once(&mut subject_id, pair[1].to_str().ok_or_else(usage)?.to_owned())?;
                }
                "--request-sha256" => {
                    set_once(&mut request_sha256, pair[1].to_str().ok_or_else(usage)?.to_owned())?;
                }
                _ => return Err(usage()),
            }
        }
        Ok(Self {
            request: request.ok_or_else(usage)?,
            response: response.ok_or_else(usage)?,
            cleanup_response: cleanup_response.ok_or_else(usage)?,
            subject_root: subject_root.ok_or_else(usage)?,
            package_root: package_root.ok_or_else(usage)?,
            artifact_root: artifact_root.ok_or_else(usage)?,
            subject_id: subject_id.ok_or_else(usage)?,
            request_sha256: request_sha256.ok_or_else(usage)?,
        })
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), &'static str> {
    if slot.replace(value).is_some() { Err(usage()) } else { Ok(()) }
}

const fn usage() -> &'static str {
    "usage: peritus-h2-controller --request FILE --response FILE --cleanup-response FILE --subject-root DIR --package-root DIR --artifact-root DIR --subject-id ID --request-sha256 SHA256"
}
