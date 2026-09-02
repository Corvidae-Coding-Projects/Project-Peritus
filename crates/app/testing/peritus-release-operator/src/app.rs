//! Top-level operation dispatch.

use crate::{args, error::OperatorError, evidence, publish};

pub fn run_from_env() -> Result<(), OperatorError> {
    match args::parse(std::env::args_os().skip(1))? {
        args::Operation::Generate { record } => evidence::generate(&record),
        args::Operation::Publish { record, provenance_bundle, sbom_bundle } => {
            publish::retain_and_upload(&record, &provenance_bundle, &sbom_bundle)
        }
    }
}
