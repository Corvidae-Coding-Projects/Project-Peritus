//! Deterministic, protocol-neutral fixtures for Peritus tests.
//!
//! Production crates must use this crate only as a development dependency. It intentionally owns
//! no production boundary traits or protocol envelopes.

mod clock;
mod event;
mod fault;
mod fixture;
mod ids;
mod provider;
mod repository;
mod script;
mod tool;

pub use clock::{ClockComponent, ClockError, ClockReading, FakeClock};
pub use event::{EventFixtureBuilder, EventFixtureContext, EventFixtureError};
pub use fault::{
    FaultControlError, FaultExpectation, FaultHit, FaultInjector, FaultLabel, FaultNameError,
    FaultPlan, FaultPlanError, FaultPoint, FaultSnapshot, FaultVerificationError,
};
pub use fixture::{
    CompatibilityCoverage, CompatibilityPolicy, FixtureCase, FixtureCatalog, FixtureError,
    FixtureErrorKind, FixtureFile, FixtureKind, FixtureManifest, FixtureName, FixturePath,
    FixtureVersion,
};
pub use ids::{DeterministicIdSource, IdSourceError};
pub use provider::{FakeProvider, ProviderScriptError};
pub use repository::{
    FixtureSymlinkKind, GitCommandOutput, GitObjectId, TempRepositoryError,
    TempRepositoryErrorKind, TemporaryRepository, TemporaryRepositoryBuilder,
};
pub use script::{
    ExpectedCall, ObservedCall, ScriptIncomplete, ScriptViolation, ScriptViolationKind,
    ScriptedCalls, ScriptedStream, StreamError,
};
pub use tool::{FakeTool, ToolScriptError};
