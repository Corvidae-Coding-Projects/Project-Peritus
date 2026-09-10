//! Typed launch profiles, scoped capture receipts, and independently classified result evidence.

use crate::{AppErrorCode, AppProtocolError};

/// Maximum number of literal arguments in one launch profile.
pub const MAX_WORKBENCH_LAUNCH_ARGUMENTS: usize = 256;
/// Maximum number of required environment references in one launch profile.
pub const MAX_WORKBENCH_LAUNCH_ENVIRONMENT: usize = 64;
/// Maximum number of launches retained in one result page.
pub const MAX_WORKBENCH_LAUNCHES: usize = 16;
/// Maximum interactions retained for one launch.
pub const MAX_WORKBENCH_INTERACTIONS: usize = 256;
/// Maximum captures retained for one launch.
pub const MAX_WORKBENCH_CAPTURES: usize = 64;
/// Maximum feedback rows retained for one launch.
pub const MAX_WORKBENCH_ARTIFACT_FEEDBACK: usize = 256;

const MAX_LAUNCH_TEXT_BYTES: usize = 4096;
const MAX_PREVIEW_INPUT_BYTES: usize = 64 * 1024;
const MAX_LAUNCH_WALL_MILLIS: u64 = 600_000;

mod profile;
pub use profile::*;
mod capture;
pub use capture::*;
mod result;
pub use result::*;

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

#[cfg(test)]
mod tests;
