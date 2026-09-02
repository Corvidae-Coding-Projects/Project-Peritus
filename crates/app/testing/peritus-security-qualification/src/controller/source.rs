//! Exact candidate-source identity and bounded file observations.

use std::path::{Path, PathBuf};

use crate::hex_digest;
use crate::repository::CandidateRepository;

use super::error::ControllerError;

pub(super) fn canonical_candidate_root(path: &Path) -> Result<PathBuf, ControllerError> {
    Ok(CandidateRepository::open(path)?.into_root())
}

pub(super) fn verify_source_digest(root: &Path, expected: &str) -> Result<String, ControllerError> {
    let repository = CandidateRepository::open(root)?;
    repository.verify_clean()?;
    let actual = hex_digest(repository.source_digest()?);
    if actual != expected {
        return Err(ControllerError::protocol(
            "git archive HEAD does not match the request candidate source digest",
        ));
    }
    Ok(actual)
}
