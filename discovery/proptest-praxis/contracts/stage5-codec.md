# Stage 5 canonical primitive and frame consumer

The `codec` target directly calls public `peritus-codec` frame, primitive reader and transactional primitive writer APIs. It needs no database, model inference or external network. Source implementation is prepared; native checks and deliberate mutation calibration remain coordinated parent-owned steps.

Read the codec package/source maps, active manifest, `frame.rs`, `limits.rs`, `message.rs`, `reader.rs`, `writer.rs` and `error.rs`; application dispatcher/request/control callers show current nested decode and complete-consumption usage. Expected bytes never come from the production encoder or decoder.

The reference is an ordinary byte list, numeric big-endian fold and explicit cursor. Writers compare the entire prefix after every successful or rejected primitive and verify rejection leaves it unchanged. Readers compare each value, error kind/limit/offset, consumed cursor and remaining bytes. A failed length header may already be consumed; a rejected invalid boolean/option consumes its tag, and invalid UTF-8 consumes its declared bytes. The checker does not invent rollback for readers. Collections, opaque fields, Unicode strings, booleans/options, fixed bytes and unsigned widths are covered. Constructed value tuples supply valid length-bearing fields, alongside arbitrary malformed bytes and correlated truncations.

Frames have independently built literal magic/version/family/schema/flags/length and exact source payload. Encode limits and field precedence are checked separately from decode grammar. Malformed partitions cover magic, unsupported format, zero family/schema, reserved flags, truncation at every selected header/body position, trailing bytes and oversized declared payload. Complete-frame capacity is checked immediately below, exactly at and above the actual input length without allocating the represented maximum. Payload/frame error precedence follows the public contract and retained source; no huge ceiling allocation is used.

Nested callbacks use bounded levels and explicit callback failure. Both reader/writer must retain the exact accepted prefix/cursor and reset depth after success or error, permitting another aggregate. Callback-wide rollback is not promised by the implementation and is not asserted.

Cases are exact serde values. Histories cap at64 operations, constructed read tuples at8 values, frame payloads at128 bytes and field/reader replay representations at small stated bounds. Counters separate primitive successes/rejections, atomic writer failures, invalid grammar partitions, frame capacity boundaries, constructed tuples/truncations and continued nested use. These bounds are exploration limits, not product limits.

`stage5-codec.patch` makes an invalid boolean tag decode as false in the real reader. `calibration()` supplies tag2, and a broad generator branch deliberately supplies arbitrary boolean/option tags. The independent closed-tag oracle must fail; parent must retain successful mutant compilation, generated failures under two seeds and restored exact/shrunk replays.

This target does not qualify arbitrary custom message implementations, all application DTO semantic fields, multithreaded I/O, cryptographic strength or allocation behavior at multi-GiB representation ceilings. Application wire families and retained compatibility data have a separate target.
