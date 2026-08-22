//! Hardened temporary Git repositories rooted at caller-selected paths.

mod command;
mod environment;
mod failure;
mod filesystem;
mod owned;

pub use command::{GitCommandOutput, GitObjectId};
pub use failure::{TempRepositoryError, TempRepositoryErrorKind};
pub use owned::{FixtureSymlinkKind, TemporaryRepository, TemporaryRepositoryBuilder};
