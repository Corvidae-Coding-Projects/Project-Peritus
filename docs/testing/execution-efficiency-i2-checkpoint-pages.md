# I2 completion: immutable checkpoint-page reuse

Status: complete for I2, phase three. I0–I2 are complete; I3–I6 remain incomplete.
Verified page reuse is now enabled by default. Setting `reuse_checkpoint_pages = false` under
`[context.local]` retains the original path against current history.

## Implementation and preserved boundaries

The page cache binds immutable C6 observation-page identities to lineage, exact record ranges,
and the existing canonical working/source artifacts. Changed tails are encoded normally.
Restoration rebuilds descriptors from a validated checkpoint. Every reused artifact still
receives full size/digest verification and every checkpoint keeps its complete dependency roots.
The compiled encoder and existing page kind/layout determine encoding identity; no cache is
persisted as another authority. Both batch and request-admission checkpoints remain.

I2 adds explicit range checks, encoding/reuse diagnostics and the qualified default. It retains
fsync, generation CAS, previous links, trace cursors, persist-before-observe ordering, current
role/permission/source checks and independent acceptance. It introduces no checkpoint or C6
wire version. Unchanged source pages avoid duplicate encoding, temporary writes and finalization;
headers, entries, protocol, transcript and manifests can still require encoding. Full verification
and root enumeration remain visible costs; total work is not claimed proportional only to edits.

A real warm-cache corruption replay exposed an existing verifier inconsistency: `read()` required
a regular artifact, while `verify()` hashed through a symlink. The prior binary accepted a
symlink to matching bytes during publication in both selections. The shared hashing boundary now
checks object type first, aligning verification/finalization with the reader. Symlinks,
directories and FIFOs reject without entering their content stream. The replay is retained with
`--expect-bug` for the preserved prior binary. Supported regular-file behavior remains intact.

## Matched real-operation measurements

Two real public context consumers operate identical fixture-owned files in separate C0 stores.
Each grows past two sealed 512-record pages to 1,139 retained observations. Seed and measured
operations alternate configuration order; each measured pair executes a real read/tool batch,
its required checkpoint, and next-view assembly/publication. Every ordered text/tool view,
including arguments and outcomes, and every canonical component digest matches across modes.

All 190 published checkpoints have independently checked complete roots and valid artifact
bytes. The source oracle checks 4,128 delivered page occurrences against the 32 fixture-owned
files and SHA-256 identities. No inference substitute or agent-written unit suite is used.

| Measured work across 32 operation pairs / 64 checkpoints per mode | Original | Reuse |
| --- | ---: | ---: |
| Checkpoint artifact bytes written | 30,877,714 | 2,222,028 |
| Artifact finalizations | 960 | 320 |
| Observation-page encodings | 192 | 32 |
| Archive-index-page encodings | 192 | 32 |
| Verified observation-page pairs reused | 0 | 160 |
| Local API median, ms | 617.206 | 424.570 |
| Local API range, ms | 568.211–728.145 | 386.240–486.703 |

Checkpoint artifact writes fall 92.804%, finalizations 66.667%, and the matched local API median 31.211%.
All 32 measured pairs favor reuse. These are Linux debug-build observations with warm OS caches,
including validation/publication, not a universal application or provider-latency guarantee.
Write/finalization totals are publication-boundary deltas; tool-event writes and SQLite traffic
are excluded from those totals.

The original whole-corpus timing regression was reproduced, then reversed by reversing order:
original/reuse took 37.725/70.059 seconds in forward order; reuse/original took 43.819/66.746 in
reverse order. The second whole cohort was slower in both cases. Those order-sensitive samples
are retained but excluded from the improvement estimate. The matched, alternating replay avoids
that whole-cohort comparison and retains every sample.

A separate `strace` replay observes actual artifact `openat`/`write`/fsync/publication syscalls.
The four selected sealed artifacts are each created/written once with reuse; original execution
creates their temporary copies 26 or 10 times. Kernel-observed temporary writes for those four
artifacts total 414,307 bytes with reuse versus 7,452,958 originally. Eight unchanged next-view
checkpoints encode zero observation/index pages with reuse. This trace is excluded from latency
measurements. Byte totals describe artifact writes, not all device traffic or SQLite WAL writes.

## Corruption, interruption and recovery

Fourteen active-owner corruption cases cover deletion, truncation, same-size/restored-mtime
corruption, replacement, matching-byte symlinks, directories and FIFOs in both selections.
Optimized reuse rejects every damaged/non-regular case without publishing a new root. The
original full encoder retains its existing ability to regenerate an exact missing object from
its authoritative in-memory state; that case is checked for exact repaired bytes. Other invalid
objects reject. Eight fresh-process integrity cases also pass, including a preserved older
binary, reuse-disabled continuation, exact roots and corrupted-page rejection.

Fourteen interruption/failure cases exercise both selections. GDB stops the actual native program
before artifact synchronization/finalization, before checkpoint CAS, and immediately after CAS
before the caller observes publication; its stack/location witnesses are retained, and the
process is killed without cleanup. Separate kills occur after a journal-durable batch and
after an active-profile batch checkpoint, before the next request. Actual SQLite triggers force
both batch and request checkpoint publication to abort atomically.

Fresh owners recover committed observations without redispatching a read or provider request.
Unobserved source data stays absent; completed reads retain the exact changed fixture marker.
Only fully committed publication/batch boundaries change the committed root. Failed publication preserves
the prior root, and subsequent recovery succeeds. These are process-crash cases, not power-cut
qualification. The varied API campaign additionally passes nine groups/264 actions covering
source changes, Unicode paging, authority changes and pending/settled ordering under the new default.

## Real provider and product acceptance

Actual Codex `gpt-6.1-sol` runs complete chat source reads plus a real `context_update` in both
selections. They return the exact independent JSON answer and preserve every source byte.
Initial canonical chat views match. Two managed CSV edits both reach `Accepted` through the
existing deterministic gate and independent reviewer; exact CSV bytes, original Git HEAD/index,
manifest and file set are independently checked. Writer/reviewer roles are observed in the trace. Independent root/privacy inspection also
passes for all 31 native checkpoints across six role lineages.

| Journey | Reuse | Provider requests | Input | Cached input | Output | Elapsed seconds |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| chat | off | 3 | 47,060 | 15,872 | 402 | 24.140 |
| chat | on | 3 | 47,017 | 7,936 | 361 | 25.255 |
| managed | on | 7 | 146,260 | 17,920 | 986 | 58.111 |
| managed | off | 6 | 124,605 | 9,984 | 966 | 50.522 |

Canonical C5 per-response high-water totals independently reconcile with cumulative accounting.
Missing optional reasoning/cost counters remain unknown. All attempts count. Different model
continuations produce different request counts; these single native pairs verify functionality,
not token savings or provider speed. The more expensive managed reuse run is retained above.
No source bodies, fixture markers or workspace paths appear in efficiency diagnostics.

## Evidence, source and rollback

Evidence root: `/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`.

- `i2-qualified-paired/result.json`: matched local operations, samples, encodings and roots.
- `i2-qualified-physical/result.json`: real syscall observations, raw traces and page writes.
- `i2-symlink-before/result.json`: the preserved prior-binary defect in both selections.
- `i2-qualified-warm-corruption/result.json`: fourteen active-cache cases.
- `i2-qualified-interruption-r3/result.json`: fourteen process/publication cases and GDB witnesses.
- `i2-qualified-integrity/result.json`: eight fresh/older-reader integrity cases.
- `i2-qualified-live/result.json`: four real-provider cases, both managed edits accepted.
- `i2-qualified-varied/result.json`: nine retained varied API groups/264 actions.
- `i2-qualified-build.log`, `i2-qualified-clippy.log`, `i2-qualified-fmt.log`: pinned locked
  daemon/CLI/TUI build, affected all-target/all-feature strict Clippy and workspace formatting.
  `git diff --check` passes. Cargo uses two jobs.
- `i2-final-source-identity.json` and `i2-final-binaries.json`: exact provenance. The source hash
  inventory is not whole-codebase reading coverage.

The reviewed path is context-port batch/assembly → checkpoint/pages/local store → artifact
verification/finalization and C0 append, plus C6 observation-page identity/encoding/recovery.
No public API or schema migration is required. Explicitly disabling page reuse reopens current
history, retains command ownership and preserves all current observations. Linux/Codex is the
live qualified combination; other platforms/providers and later increments remain unqualified.
No repository commit, push or deployment is part of this completion.

Architecture verdict: **ready for I2**.
