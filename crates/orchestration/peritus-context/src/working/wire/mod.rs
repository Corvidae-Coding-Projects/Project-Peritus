//! Versioned C0-canonical runtime encoding; deterministic semantics remain in the reducers.

mod entry;
mod event;
mod fields;
mod pages;
mod protocol;
mod state;

pub use event::{decode_working_event, encode_working_event};
pub use pages::{WorkingStatePageDecoder, WorkingStatePages};
pub use state::{decode_working_state, encode_working_state};

use super::WorkingError;
use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecLimits};

const LIMITS: CodecLimits = CodecLimits::new(
    usize::MAX,
    usize::MAX,
    u32::MAX as usize,
    u32::MAX as usize,
    u32::MAX as usize,
    8,
);
const VERSION: u16 = 2;
const HOST_VERSION: u16 = 3;
const HOST_ENTRY_VERSION: u16 = 4;

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
    fn from(error: CodecError) -> Self {
        Self::Codec(error)
    }
}
impl From<WorkingError> for WorkingCodecError {
    fn from(error: WorkingError) -> Self {
        Self::State(error)
    }
}
impl core::fmt::Display for WorkingCodecError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "working codec rejection: {self:?}")
    }
}
impl std::error::Error for WorkingCodecError {}

fn writer(magic: [u8; 4]) -> Result<CanonicalWriter, WorkingCodecError> {
    writer_version(magic, VERSION)
}
fn writer_version(magic: [u8; 4], version: u16) -> Result<CanonicalWriter, WorkingCodecError> {
    let mut writer = CanonicalWriter::new(LIMITS);
    writer.write_fixed(&magic)?;
    writer.write_u16(version)?;
    Ok(writer)
}

fn reader_version(
    bytes: &[u8],
    magic: [u8; 4],
) -> Result<(CanonicalReader<'_>, u16), WorkingCodecError> {
    let mut reader = CanonicalReader::new(bytes, LIMITS);
    if reader.read_fixed::<4>()? != magic {
        return Err(WorkingCodecError::InvalidValue);
    }
    let version = reader.read_u16()?;
    if !matches!(version, 1 | VERSION | HOST_VERSION | HOST_ENTRY_VERSION) {
        return Err(WorkingCodecError::InvalidValue);
    }
    Ok((reader, version))
}

fn count(reader: &mut CanonicalReader<'_>, maximum: usize) -> Result<usize, WorkingCodecError> {
    let count = reader.read_collection_len()?;
    checked_count(reader, count, maximum)
}

fn wide_count(
    reader: &mut CanonicalReader<'_>,
    maximum: usize,
) -> Result<usize, WorkingCodecError> {
    let count = usize::try_from(reader.read_u64()?).map_err(|_| WorkingCodecError::InvalidValue)?;
    checked_count(reader, count, maximum)
}

fn checked_count(
    reader: &CanonicalReader<'_>,
    count: usize,
    maximum: usize,
) -> Result<usize, WorkingCodecError> {
    // Every collection element consumes at least one byte. Never allocate from an unchecked prefix.
    if count > maximum || count > reader.remaining() {
        Err(WorkingError::Capacity.into())
    } else {
        Ok(count)
    }
}
