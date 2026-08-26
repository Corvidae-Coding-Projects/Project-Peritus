//! Deterministic application-protocol schema and compatibility artifact generation.

mod codegen;
mod fields;
mod fixtures;
mod registry;
mod render;

pub use codegen::run_codegen;
pub use fields::{
    APP_FLOW_TYPES, APP_NESTED_TYPES, AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType,
    FieldBound, JsonShape,
};
pub use fixtures::{FixtureClass, GeneratedFixtureCase, generated_fixture_cases};
pub use registry::{
    APP_ERROR_CODES, APP_FAMILIES, AppFamilyDescriptor, AppPayloadDescriptor, ErrorCodeDescriptor,
};
pub use render::{GeneratedTextArtifact, generated_text_artifacts};
