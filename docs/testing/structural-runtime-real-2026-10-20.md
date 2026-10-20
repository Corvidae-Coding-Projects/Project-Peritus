# Structural runtime remediation: real operation evidence

Scope: the six source-review findings at `5f808cd092dbddd4cc158ff6412b7d572db2817f`.
The original source and three original executables were retained in an isolated worktree/binary
folder. This campaign used ordinary executable builds, native consumers of production APIs,
real launcher/TUI/CLI/daemon processes, SQLite commits, native plugin subprocesses, and copied
previously governed run state. No unit tests, mock suites, formal verification, or repository
qualification suites were created or run. Live user/builder daemons and installed binaries were
not changed.

## Implemented boundaries

| Finding | Change | Behavioral evidence |
| --- | --- | --- |
| Shared registry lock spans run persistence | Exact-key writer ownership and immutable published views; workspace admission remains serialized independently | A deliberately held run-A transaction does not block reading A's last published state or durably updating B; another A writer waits and receives the preceding update. 256 concurrent updates retain every value. |
| One conversation replaces another's replay cache | Independently retained verified conversations, LRU eviction, and fenced advancement of unchanged entries after an exact host commit | Six alternating conversations, 72 new durable inputs, 72 exact-operation replays, 432 queries, and reconnections preserve revisions and receipts. Six initial cold replays, 519 cache hits. |
| Startup eagerly hydrates history | Disposable stamped indexes and demand-loaded full records/continuations; exact start/control/candidate validation before use | Original governed continuation loads unchanged. Warm startup does not retain the synthetic archival payload. Broken, semantically invalid, and stale indexes rebuild from source. Missing/corrupt continuations are rejected when selected; repairing the source allows a retry within that same process. |
| Clients depend on the agent runner for small helpers | Text and presentation contracts move to product-state; candidate ownership moves to workspace; trace decoding moves to trace | Original and extracted candidate identity APIs match across ten real Git states. Actual CLI reads all 791 complete frames of a copied native trace. Ordinary TUI/CLI behavior is exercised below. Compatibility exports preserve existing runner API callers. |
| Plugin stderr disappears | Continuous bounded drainage, observed process status, and a terminal-safe causal tail in transport errors | Native success, exit 23, 2 MiB stderr flood followed by exit 23, malformed framing, and timeout. Every child is reaped; failure reasons remain visible. |
| Plugin dependencies violate layer ownership | SDK physically moves to foundation, host to runtime; workspace/manifests/maps follow the owners | Actual native SDK/host process exchange succeeds. Current production manifest edges have no forbidden layer direction. The application-to-extensions prohibition remains intact. |

Cache eviction only discards reusable in-memory verification. It cannot reject work, truncate
history, or impose a task/token budget. Indexes are optimization hints, never execution authority.
Full selected records still validate their governing receipts, continuation digests, and lineage.
Potentially live command owners are still inspected during startup; completed historical command
receipts do not force continuation hydration.

## Replays and independent expectations

Native consumers and executable fixtures are in
[structural-runtime/](reproducers/structural-runtime/AGENTS.md). They exercise these production
interfaces directly, rather than reconstructing replacement implementations.

- Publication: independent-key progress while a same-key writer is held, durable bytes before
  publication, same-key ordering, stable old views, concurrent updates, one hydration across
  concurrent readers, and retry after a failed loader. Independent work completed in 0.007 ms
  in the recorded run while A remained deliberately held.
- Conversation cache: exact revisions/titles after every interleaved append, original receipt
  replay, and fresh client connections. A separate real SQLite connection then committed an
  unrelated schema change: the next read replayed. Deliberately corrupting a state row was
  rejected rather than served from cache. Restoring the row allowed a fresh successful read.
- Candidate identity: clean and modified contents, executable-bit changes, a symlink, a Unicode
  untracked path, staged changes, a nested repository, a modified nested repository, ignored
  build output, and a tracked deletion. All ten matched the pre-change production API exactly.
- Trace: the actual JSON CLI emitted 791 records, matching an independent header/length walk of
  the copied trace. The original trace hash remained unchanged.
- Restart: eight baseline/candidate/corruption cases. No provider was dispatched. The full
  selected summary remained exact. Failed continuation loads quarantined the unusable
  projection, retained its evidence, and recovered after repair without restarting the daemon.
- Plugin noise uncovered a second defect during this campaign: the existing 1024-byte error
  formatter cut off the final crash message after a long retained tail. The transport now
  reserves space for the causal tail. The 2,097,205-byte stderr case retains a 16 KiB ring and
  returns a 555-byte error containing the final failure and exit status. The malformed case
  explicitly asserts a protocol failure; the timeout asserts a timeout failure.
- The existing real launcher/shutdown replay passed 28 cases on the changed CLI, including
  separate installations, Unicode/long paths, explicit endpoints, concurrent shutdown,
  retained original workspace bytes, and rejection of an unknown session.

The expanded TUI replay requires a configured writer to preview attachments. An initial offline
invocation reached eight local display/editor/key/theme checks and then stopped at that unmet
prerequisite. That incomplete run is retained and is not counted as an attachment pass. The
configured replay uses `--provider --local-only`: it configures the real provider route and
exercises local controls without requesting model inference. It passed 56 scenarios, including project/external-file preview, draft retention, clipboard image preview, queue controls, editor/key/theme controls, generated modal sequences, and restart. No inference was requested.

## Initial measurements

Equivalent original/changed developer-profile executables use optimization level 1 and no debug
symbols. Compilation was bounded to two workers, three CPU equivalents, MemoryHigh 2500 MiB,
and MemoryMax 3200 MiB. Native workloads were small and isolated.

The six-conversation IPC sample measured median query latency of 0.231 ms before and 0.015 ms
after; p95 was 0.503 ms before and 0.028 ms after. Median durable input operation latency was
1.815 ms before and 1.613 ms after. These are local host measurements, not provider latency.

The startup fixture combines a real governed continuation with a synthetic 15.5 MiB retained
finding-state field. Initial index migration still reads legacy JSON once. Warm startup reads a
roughly 4.4 KiB index and defers the full selected payload. The first explicitly selected history
item therefore pays the load cost; this is preserved functionality, not discarded history.
Repeated timings and memory review follow the implementation commit.

This is bounded evidence for the changed paths, not exhaustive application coverage or a
multi-hour leak/soak claim. Linux native process execution was exercised; macOS, Windows, and
Wasm execution were not run on this machine.

Raw source worktree, binaries, logs, exact observations, copied state, and JSON results:
`/home/doll/.cache/peritus-structural-20261020/`. Key artifacts are `binaries.json`,
`publication.jsonl`, `candidate-cache.json`, `baseline-cache.json`, `identity-results.json`,
`trace-results.json`, `history-copy/history-results.json`,
`candidate-live/invalidation-results.json`, `plugins-final.jsonl`, `cli-replay/results.json`,
and the configured TUI replay's `results.json`. These generated artifacts remain outside Git.
