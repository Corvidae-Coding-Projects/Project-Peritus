# Framing CRLF boundary discovery

- Source under test: `f75f5dee3976b1c72b85e78fa1d6c50475d5d8fa`
- Host: Linux x86_64
- Classification: confirmed product defect
- Boundary owner: `peritus-provider-core`
- Oracle: decoding one complete frame is invariant under every split of the same wire bytes

An SSE line or NDJSON record whose content was exactly `max_frame_bytes` was rejected when a
carriage return arrived at the end of one chunk and its line feed arrived in the next chunk. The
pending-byte admission check counted the possible CRLF delimiter byte as content before the
parser could see the LF.

The focused `framing_partitions` regression reproduced from three fresh test processes. Each run
had the same logical signature: four boundary assertions failed and the hard-buffer negative
control passed. The minimized checked-in corpus entries are `corpus/sse/exact-limit-crlf` and
`corpus/ndjson/exact-limit-crlf`; their first two bytes select the exact frame limit and a chunk
width that separates CR from LF.

The fix excludes only one trailing carriage return from the pending content-size check. The
independent buffer ceiling is unchanged, and a following non-LF byte makes that carriage return
ordinary content again. The regression now passes across every split position, verifies an
interior CR cannot widen the limit, verifies the total buffer ceiling, and checks final unterminated
CR behavior. The stable corpus replay and strict Clippy checks also pass.

The test used no network, provider credentials, external process, or persistent user state. Its
only retained files are the source regression, minimized synthetic corpus entries, and this
evidence record.
