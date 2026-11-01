# FRAMING pilot contract

Status: Stage 1 build, fixed replay, two standard generated searches, mutation detection/shrinking, restored replay, strict lint and formatting passed. Exact observations and tested source/executable identities are in `../REPORT.md` and `../evidence/stage1.json`; this contract does not extend those observations beyond the stated pilot scope.

## Actual owner and callers read

- `crates/model/peritus-provider-core/Cargo.toml`, `src/lib.rs`, `src/error.rs`, `src/framing.rs`, and all of `src/framing/{sse,ndjson}.rs`: public limits, constructors, typed errors, incremental push, complete-line processing, finalization, and post-finalization rejection.
- `crates/model/peritus-provider-openai/src/stream.rs` and `crates/model/peritus-provider-compatible/src/stream.rs`: these own an SSE parser under their provider stream and consume bounded normalized frames; provider-level semantic decoding is a separate target.
- Sibling call sites in the Google and Anthropic streams were located. Their full semantic decoders were not reviewed or exercised by this pilot. NDJSON's public API is exercised directly; no active model-adapter caller was found in the bounded source search.
- Applicable package/source/framing `AGENTS.md` maps were read. Source at `d106c9b13318fbcdd3d816ed63ae48532806fa7e` establishes the implementation under investigation.

## Independent oracle

SSE starts from explicit semantic records: event name, optional ID, a nonempty list of data lines, comment, exact done sentinel, or an ignored metadata-only block. A small writer constructs bytes and retains the known semantic records. The oracle never calls a production parser, frame decoder, or framing helper. NUL-containing IDs have an explicitly absent expected ID. Every frame's metadata is local to that block under the repository's current contract; this pilot does not claim full WHATWG EventSource compatibility.

NDJSON starts from JSON string values serialized with `serde_json`, with independently retained exact record strings. Extra blank lines and CRLF are constructed separately. Boundary cases intentionally use non-JSON ASCII records: the public NDJSON framer promises line framing and UTF-8 validation, not JSON semantic validation.

Every successful replay checks exact records in order, missing/extra records, and unchanged Unicode text across whole-stream, byte-at-a-time, generated mixed and empty chunks, and every two-part split for wires up to 128 bytes. The total wire fits the pending-buffer allowance for these comparisons. This is required because a `push` checks pending-plus-incoming length before draining lines; an oversized one-shot incoming chunk is independently allowed to fail while smaller chunks succeed.

## Input families and counters

1. Valid mixed SSE blocks and JSON-string records: Unicode code points, combining scalar, NUL, escapes, whitespace, ignored metadata, comments, exact `[DONE]`, multiline data, empty wire, empty string, LF/CRLF, and final unterminated line/event.
2. Frame bytes at limit minus one, exact limit, and limit plus one, independently crossing wire family, LF/CRLF, and terminated/unterminated input.
3. Invalid UTF-8 in a completed or final unterminated line. The retained incomplete leading byte cannot become valid through changed chunk boundaries.
4. One incoming chunk beyond the buffer ceiling, paired with exact-buffer acceptance using blank lines to isolate admission from logical-frame validation.
5. Invalid zero, inconsistent, and widened production limits.
6. Every successful schedule also attempts repeat `finish` and empty `push` after finish and requires `MalformedStream`.

Metrics include actual pushes, bytes fed, empty pushes, schedules, emitted items, successful schedules, expected rejections, finishes, UTF-8/CRLF split points, per-family/delimiter/unterminated partitions, exact frame boundary classes, buffer acceptance/rejection, and invalid-limit rejection. Generated cases use constructive strategies with no filters or hidden replay randomness. Serialized `Case` values preserve exact text, configuration, and chunk schedule independently of a seed.

## Mutation calibration

`calibration/framing.patch` changes the NDJSON completed-line frame bound to the wider pending-buffer bound. `calibration_case()` supplies a 9-byte record followed by CRLF under an 8-byte frame ceiling and a larger buffer. The correct parser must reject it with `LimitExceeded`; the mutation accepts it and the harness must fail. Apply only to an isolated calibration checkout, retain baseline/canary/restored outcomes, and revert before integration. A compile failure is not a behavioral catch. No production source has been changed by this worker.

## Explicit gaps

This pilot does not yet exercise provider JSON semantics, HTTP cancellation/backpressure, process crashes, arbitrary raw-byte acceptance beyond the targeted malformed family, parser behavior after a failed `push` (the API specifies no continued-use contract), maximum production-size allocations, all SSE field overwrite/unknown-field combinations, or exhaustive multisplit partition enumeration for long wires. It does not prove the repository or the parser bug-free. Mutation sensitivity and observed generated distribution require actual run evidence before this target can be marked qualified.
