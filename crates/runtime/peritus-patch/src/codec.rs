//! Native representation capacities for local canonical records.

/// Local records retain native `u32` length prefixes and the host allocation bound.
pub const fn local_record_codec_limits() -> peritus_codec::CodecLimits {
    peritus_codec::CodecLimits::new(
        usize::MAX,
        usize::MAX,
        u32::MAX as usize,
        u32::MAX as usize,
        u32::MAX as usize,
        64,
    )
}
