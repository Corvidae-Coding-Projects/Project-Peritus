# Stage 4: owned artifact streaming integrity

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

## Contract and real interface

The public consumer uses actual `ArtifactStore::begin_owned_write` / `complete_write`,
SQLite-backed metadata, verified `read` and `open_read`, and `ArtifactReadHandle::read_chunk`.
Its files and database live in a fresh owned temporary directory per exact case.

Owned-reader docs promise rejection of content mutation. The fulfilled full-stream promise
is that bytes successfully returned through completion match the advertised original size
and SHA-256 digest. Earlier chunks are provisional. There is no per-chunk authentication,
snapshot, locking, concurrent-writer, post-completion filesystem-integrity or zero-exposed-
partial-bytes guarantee. Modifying already-consumed bytes alone can still complete correctly
when the entire actually returned stream is original. The fixture uses deterministic sequential
external writes between public calls, not an unproven normal store API that overwrites finalized
objects. All generated XOR changes have a nonzero mask.

The repaired owner computes a rolling stream digest, checks it before returning the final chunk
or an empty-stream successful EOF, retains success for repeated EOF, and poisons stream I/O or
integrity failure. A partial I/O read cannot be resumed safely. Invalid zero/oversized byte bounds
remain reversible and never advance the public cursor. Error retries cannot become successful
EOF, even if the external file is repaired. The existing cursor behavior on an initial terminal
failure is retained; the oracle does not invent a rollback obligation for a rejected read.

## Sources and callers read

- `peritus-artifact-store/src/owned.rs`, `store.rs`, `writer.rs`, `finalize.rs`, `path.rs`,
  `error.rs`, crate README and `docs/c0-durable-state.md`: publication, preverification,
  streaming bounds, exact metadata and corruption semantics.
- Daemon `artifact/service.rs:open_download,poll_download` and A3 `ArtifactTransferState`:
  the sender relies on the reader; the protocol state only counts/compares supplied observations.
  `docs/a3-app-protocol.md:95-97` requires a matching incremental digest observation.
- Daemon `artifact/service/scoped.rs:read_scoped`, RSI `execution/materialize.rs:copy_artifact`
  and `evaluation.rs:read`: these consumers independently hash completed bytes.
- CLI `artifact.rs` download similarly hashes bytes before publishing the output.
- Product-runner `developer_tools/command_runtime/result.rs:output_prefix` may read only a
  provisional prefix and is not evidence of authenticated whole-stream completion.

No caller interface changes are needed. Native daemon transfers and CLI flows were not exercised
in this target; their wiring implications are source-grounded. General I/O/permission fault
injection is not claimed: real short reads and unexpected growth exercise sticky failures.

## Independent oracle and generated histories

The independent fixture keeps three byte lists: immutable original input, current physical-object
contents, and actual bytes returned to the consumer. No production verification/reducer helper
calculates expectations. Each returned chunk must match the independently maintained current
object slice at the conserved output offset and satisfy its requested bound. On successful EOF,
observed bytes and a direct SHA-256 calculation must equal the original input and original hash.
Faulted streams may reject before completion; uncorrupted streams must complete. Invalid bounds
must reject without cursor changes; valid reads after terminal errors must continue rejecting.

Operations are `Read(bound)`, `Xor(position,nonzero_mask)`, `Resize(size)`, `Append(byte)` and
`Restore`. Real in-place file writes and syncs reproduce the intended physical byte list. The
generator combines broad mixed histories with constructive after-open/before-first corruption,
and correlated first-read prefix lengths plus mutation positions chosen inside the consumed
prefix or still unread suffix. Thus those important interactions remain supported during
shrinking instead of depending on rare independent random operands. Pre-open corruption is a
separate low-weight branch. Payloads include empty, small binary content and the exact 256-byte
configured boundary. Bounds cover zero, one, the exact ceiling and one byte above it. Every case
drains with a positive bound, probes invalid bounds, restores the file and retries terminal state.

Exact serde cases replay without hidden randomness. Inputs are capped at 256 original bytes and
64 explicit operations; generation honors `max_steps` by limiting/truncating operation lists.
Metrics count verified opens, real returned chunks, completed streams, unread/consumed-prefix
overwrites, length changes, pre-open rejections, detected same-size faults, sticky-error retries,
reversible bound rejections and repeated EOF. Metrics reflect actual operations/observations.

## Calibration and evidence

`calibration/stage4-artifact_stream.patch` disables only the terminal rolling-digest comparison.
It leaves pre-open hashing, length checks, state poisoning and all other owners unchanged. The
explicit witness changes one unread byte without changing size; generated before-first/unread
suffix histories reach the same comparison. The independent completed-byte oracle must reject
the mutant when successful EOF returns altered content. Root owns final three-leg calibration.

Focused fix branch: `fix/artifact-stream-terminal-integrity`, based on original `d106c9b13`.
The readable public example `examples/owned_read_integrity.rs` failed the original SDK with a
successful corrupted completion (exit 101), and passed after the owner repair (exit 0).
Actual-crate strict Clippy and all-target/all-feature tests with the original locked workspace
passed; the latter currently contains zero tests and is not substituted for the native example.
Raw before/after build/run logs, exact-case generated results and source hashes are retained in
`runs/artifact-stream-fix/`; parent integrates final runner/provenance and fresh searches.
