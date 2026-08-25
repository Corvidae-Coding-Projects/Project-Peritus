//! Reproducible schema, client declaration, and compatibility-corpus generation.

mod codegen;
mod fixtures;
mod lifecycle;
mod registry;
mod render;

pub use codegen::run_codegen;
pub use fixtures::{
    GeneratedBinaryArtifact, generated_agent_binary_artifacts, generated_binary_artifacts,
};
pub use lifecycle::{
    KERNEL_COMMAND_VARIANTS, KERNEL_ERROR_VARIANTS, KERNEL_EVENT_VARIANTS, KERNEL_SUBJECT_VARIANTS,
    LIFECYCLE_PHASE_VARIANTS, LIFECYCLE_VARIANTS, VariantSet, VariantTag,
};
pub use registry::{FAMILIES, MessageFamily};
pub use render::{GeneratedArtifact, generated_artifacts};
