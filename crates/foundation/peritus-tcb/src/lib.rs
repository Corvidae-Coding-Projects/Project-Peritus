//! Audited policy anchor for Project Peritus's Verus trust boundary.
//!
//! The A1 baseline contains no trusted occurrence. Future trusted constructs are permitted only
//! under the exact, independently reviewed manifest contract documented by this crate.

#![no_std]

mod manifests;

pub use manifests::{
    actors_manifest_path, exclusions_manifest_path, obligations_manifest_path,
    proof_impact_manifest_path, trust_manifest_path, verification_manifest_paths,
};
