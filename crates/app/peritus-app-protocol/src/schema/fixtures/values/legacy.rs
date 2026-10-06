//! Frozen peer declarations for historical compatibility evidence.

use crate::AppProtocolLimits;
use peritus_codec::CodecLimits;

pub(super) fn limits() -> AppProtocolLimits {
    // Expanding the daily-driving default must never rewrite the historical Hello fixtures.
    AppProtocolLimits::new(
        CodecLimits::LEGACY_V1,
        16,
        64,
        4096,
        64,
        256,
        256 * 1024,
        64,
        64 * 1024,
        4 * 1024,
        256,
    )
    .expect("historical compatibility profile is valid")
}
