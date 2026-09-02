//! Native file boundary for preparing and admitting signed H4 evidence.

mod admission;
mod args;
mod assemble;
mod audit_input;
mod binding;
mod build_input;
mod campaign_input;
mod criterion_input;
mod error;
mod files;
mod plan;
mod policy_input;

use std::ffi::OsString;

use peritus_release_artifacts::{BoundedId, Ed25519PublicKey, Ed25519Signature, ReleasePath};

pub use error::{OperatorError, OperatorErrorCode};

use crate::{EvidenceSignature, SignedEvidenceRecord, canonical_evidence_signature_envelope};

/// Runs the H4 evidence operator from process arguments.
///
/// # Errors
///
/// Returns a typed error for invalid arguments, unsafe files, invalid signatures, or publication
/// failure.
pub fn run_from_env() -> Result<(), OperatorError> {
    run(std::env::args_os().skip(1))
}

/// Runs the H4 evidence operator with an explicit argument sequence.
///
/// # Errors
///
/// Returns a typed error for invalid arguments, unsafe files, invalid signatures, or publication
/// failure.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), OperatorError> {
    match args::parse(args)? {
        args::Operation::Envelope(input) => prepare_envelope(&input),
        args::Operation::Verify(input) => verify_evidence(&input),
        args::Operation::Finalize(input) => assemble::finalize(&input),
    }
}

fn prepare_envelope(input: &args::EvidenceInput) -> Result<(), OperatorError> {
    let binding = binding::read(&input.binding)?;
    let payload = files::read_bounded_regular(&input.payload, "evidence payload")?;
    let retained_path = ReleasePath::new(&input.retained_path)?;
    let envelope = canonical_evidence_signature_envelope(
        &binding,
        input.kind,
        input.disposition,
        &retained_path,
        &payload,
    )?;
    files::publish_new(&input.output, &envelope)
}

fn verify_evidence(input: &args::VerificationInput) -> Result<(), OperatorError> {
    let binding = binding::read(&input.evidence.binding)?;
    let payload = files::read_bounded_regular(&input.evidence.payload, "evidence payload")?;
    let retained_path = ReleasePath::new(&input.evidence.retained_path)?;
    let public_key = files::read_exact_material::<32>(&input.public_key, "Ed25519 public key")?;
    let signature = files::read_exact_material::<64>(&input.signature, "Ed25519 signature")?;
    let record = SignedEvidenceRecord::verify(
        binding,
        input.evidence.kind,
        input.evidence.disposition,
        retained_path,
        &payload,
        EvidenceSignature::new(
            BoundedId::new(&input.key_id)?,
            Ed25519PublicKey::from_bytes(public_key),
            Ed25519Signature::from_bytes(signature),
        ),
    )?;
    let mut bytes = serde_json::to_vec(&record)?;
    bytes.push(b'\n');
    files::publish_new(&input.evidence.output, &bytes)
}
