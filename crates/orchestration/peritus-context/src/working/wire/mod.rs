//! Versioned C0-canonical runtime encoding; deterministic semantics remain in the reducers.

mod entry;
mod event;
mod fields;
mod paged_state;
mod protocol;
mod state;

pub use event::{decode_working_event, encode_working_event};
pub use paged_state::{
    EncodedWorkingStateDescriptorPage, EncodedWorkingStatePage, EncodedWorkingStatePart,
    EncodedWorkingStateSnapshot, ReusableWorkingStateHistory, WorkingStateArtifact,
    WorkingStatePageKind, WorkingStatePageReference, WorkingStateReadError,
    WorkingStateRootInspection,
    WorkingStateWriteError, decode_paged_working_state, decode_paged_working_state_from,
    decode_paged_working_state_with_history_from, encode_paged_working_state,
    encode_paged_working_state_reusing_with, encode_paged_working_state_with,
    inspect_paged_working_state_root,
};
pub use state::{
    decode_working_state, decode_working_state_core, encode_working_state,
    encode_working_state_core,
};

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecLimits};
use super::WorkingError;

// This is a bound for one PWMS/PWMC legacy snapshot or one current root, descriptor, or data
// page. PWP2 logical counts and linked page chains are deliberately not bounded by it.
const PHYSICAL_FRAME_LIMITS: CodecLimits =
    CodecLimits::new(8 * 1024 * 1024, 8 * 1024 * 1024, 65_535, 2_048, 2_048, 8);
const VERSION: u16 = 1;

/// Structural decode/encode rejection, containing no archived text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkingCodecError {
    /// Malformed, truncated, or excessive canonical data.
    Codec(CodecError),
    /// Domain validation rejected decoded state.
    State(WorkingError),
    /// Unsupported magic, version, enum tag, or invalid checked value.
    InvalidValue,
}

impl From<CodecError> for WorkingCodecError {
    fn from(error: CodecError) -> Self { Self::Codec(error) }
}
impl From<WorkingError> for WorkingCodecError {
    fn from(error: WorkingError) -> Self { Self::State(error) }
}
impl core::fmt::Display for WorkingCodecError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "working codec rejection: {self:?}")
    }
}
impl std::error::Error for WorkingCodecError {}

fn writer(magic: [u8; 4]) -> Result<CanonicalWriter, WorkingCodecError> {
    let mut writer = CanonicalWriter::new(PHYSICAL_FRAME_LIMITS);
    writer.write_fixed(&magic)?;
    writer.write_u16(VERSION)?;
    Ok(writer)
}

fn reader(bytes: &[u8], magic: [u8; 4]) -> Result<CanonicalReader<'_>, WorkingCodecError> {
    if bytes.len() > PHYSICAL_FRAME_LIMITS.max_payload_bytes {
        return Err(WorkingError::Capacity.into());
    }
    let mut reader = CanonicalReader::new(bytes, PHYSICAL_FRAME_LIMITS);
    if reader.read_fixed::<4>()? != magic || reader.read_u16()? != VERSION {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok(reader)
}

fn count(reader: &mut CanonicalReader<'_>, maximum: usize, minimum_item_bytes: usize) -> Result<usize, WorkingCodecError> {
    let count = reader.read_collection_len(minimum_item_bytes)?;
    if count > maximum { Err(WorkingError::Capacity.into()) } else { Ok(count) }
}
