//! Canonical, digest-verified compatibility fixture conventions.

mod case;
mod catalog;
mod failure;
mod manifest;
mod name;
mod path;

pub use case::FixtureCase;
pub use catalog::{CompatibilityCoverage, CompatibilityPolicy, FixtureCatalog};
pub use failure::{FixtureError, FixtureErrorKind};
pub use manifest::{FixtureFile, FixtureKind, FixtureManifest};
pub use name::{FixtureName, FixtureVersion};
pub use path::FixturePath;
