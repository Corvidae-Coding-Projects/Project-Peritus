//! Canonical versioned framing and bounded binary decoding for Peritus.

mod error;
mod frame;
mod hash;
mod limits;
mod message;
mod reader;
mod writer;

pub use error::{CodecError, CodecErrorKind, CodecLimit};
pub use frame::{
    DecodedFrame, FORMAT_VERSION, FrameHeader, HEADER_LEN, MAGIC, decode_frame, encode_frame,
};
pub use hash::{canonical_sha256, sha256};
pub use limits::CodecLimits;
pub use message::{CanonicalDecode, CanonicalEncode, decode_message, encode_message};
pub use reader::CanonicalReader;
pub use writer::CanonicalWriter;
