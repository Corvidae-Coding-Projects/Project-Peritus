//! Canonical E0 digests and protocol tag vocabulary.

pub mod digests;
pub mod wire;

pub use digests::{
    acceptance_decision_digest, acceptance_evidence_digest, certificate_digest, completion_digest,
    directive_payload_digest, evaluation_request_digest, kernel_directive_payload_digest,
    state_digest,
};
