# Checkpoint and useful-context cache campaign — 2026-10-19

Implemented on the current develop working tree, preserving the existing TUI work. All mutations
and paid probes used disposable workspaces and owned processes. Installed doll/builder binaries,
live daemons and user runs were not changed by this campaign.

## Original defects and resulting behavior

The inspected builder run had about 17,000 enrolled paths and 1.27 GiB of files, including local
PostgreSQL, Temporal and SeaweedFS state. Six sub-second successful commands were followed by
220–232-second handoffs. Repeated scope hashing, callback-level whole-candidate observations,
large checkpoint-record cloning and repeated SQLite baseline decoding were host work after the
native command had already completed. These service files remain real user data; they are not
excluded from tracking or deleted by this change.

Content observations now reuse the exact digest and preview while the Unix file's device, inode,
size, mode, nanosecond mtime and change time match. Cold reads check the opened descriptor and
pathname again before publishing reusable observations. The content cache is process-local and
approximately bounded to 64 MiB. Coarse change clocks and non-Unix hosts conservatively reread.
Review, final settlement and other acceptance-sensitive candidate observations still read fresh
content. The optimization does not claim an atomic multi-file snapshot or adversarial/network
filesystem freshness proof.

SQLite scope indexes now retain bounded immutable reader snapshots. The cache checks the actual
private-file identity, SQLite's connection-local `data_version` and immutable legacy-journal
binding. It refreshes for external enrollment or database changes. Four reader slots each retain
at most approximately 16 MiB; a cold scan holds only that index's lock. Old NUL-bearing previews
are projected as binary display without rewriting original baseline hashes or stored evidence.
Original beforeimages and scope validation remain authoritative.

Commands seal individual owned postimages, then record one candidate observation for the completed
command. Repeated checkpoint lookup/seal no-ops inspect the durable conversation without cloning
its entire record. `workspace_scope` acknowledges declarations with a tracked count rather than
returning thousands of existing paths on every declaration.

Provider input previously rebuilt working entries early, slid history on each request and changed
`provider_step` in the very first System message. Useful historical reads now remain in append-only
history epochs while they fit the negotiated provider capacity. Current scope, pending operations,
working-state projection and executor guidance follow reusable evidence, before the latest complete
exchange. Stable policy and literal user instructions stay protected. Current executor prerequisites
remain exact Developer authority; counters stay in telemetry. Resolved, superseded and stale working
entries stay durable and retrievable without being repeated as active guidance. Deduplication
preserves distinct provenance and validity. Cached read bodies require current permission and
content validation; one stale read does not discard unrelated results from its completed batch.

The Codex adapter now reuses a private cwd and an official routing-only parent. Each finite official
app-server invocation resumes that parent, creates an ephemeral prediction fork, injects the full
current request as user data and submits one structured turn. The routing frame has stable identity
and metadata; source content, tool results and current instructions still change normally. Parent
history contains native context and a constant host marker, with no Peritus task or model output.
Native duplicate memories/compaction, host skill discovery and execution features receive explicit
disabling overrides. Host tool names, arguments, output lifecycle and native-execution rejection
still pass through the existing Peritus decoder. Images use actual native inline image content.

Unavailable cache setup falls back before model submission. A rejected resume/fork/injection marks
that anchor unusable and uses ordinary isolated exec; accepted or potentially accepted inference
is never automatically replayed. Cache-disabled requests retain that ordinary path. Native model
rejections and reported usage retain their redaction-safe categories. Shared process support for
finite observer-driven protocol stdin keeps the existing byte ceilings, cancellation, timeout and
owned-child cleanup; other adapters' ordinary process requests retain their original defaults.

## Measured real behavior

| Actual interface and expectation | Observation |
| --- | --- |
| Public scope API, 26 independently driven filesystem cases, seed 8419; eight unchanged passes over 64 MiB | Original 537,986,405 logical bytes read; corrected 107. Same-size/restored-mtime edits, atomic replacement, permissions, type/symlink transitions, additions/removals, restore and concurrent observers passed. |
| Public scope API, 16,384 NUL pages of 4 KiB, old SQLite index 419,397,632 bytes; eight warm passes | Original 3,860,628,373 logical bytes; corrected 106. Observed elapsed 4.27 s original optimized consumer and 1.72 s corrected debug library; build profiles differ, so byte reduction is the clean comparison. |
| Another actual consumer enrolls existing/absent paths while first reader stays warm | New changed paths visible; altered private legacy journal rejected. |
| Coordinated external legacy append after more than 4 MiB of a cold index read | First read rejected; following warm attempt rejected. Invalid evidence is not cached as valid. |
| Actual TUI → daemon → account provider → ten edits and two finite application commands | Both Python application executions printed the independently required marker; native times 87/47 ms, command handoffs 196/153 ms. All 256 unrelated runtime file hashes unchanged. |
| Restart that fixture daemon, inspect all ten automatic checkpoints through public IPC, rewind one file | Exact original/postchange SHA-256 values retained; selected original restored, other nine edits preserved. |
| Real context assembly/host tools | Six broader reuse/freshness/privacy/isolation cases, two mixed-batch/order regressions, and three exact current-state views passed. |
| Official Codex route, unchanged 94,239-byte source corpus, changed final marker, four requests | Input 29,128 tokens each; cached 0 then 28,928 on each warm request, 99.31%. All four answers matched the latest marker. Cold 5.03 s; warm 3.57–3.76 s at fixture-only low effort. |
| Actual six-request TUI/daemon working-memory operation | Both distinct-source observations retained, resolved/superseded entries absent from active view but retrievable, sources unchanged; 32.87 s. Actual input 76,548, cached 34,048, 44.48% including cold; request two 95.44%. Later requests reported 5,760 cached tokens each. |
| Real official-client unsupported model | `InvalidRequest`, `openai.codex_runtime.model_unavailable`, no generic-envelope masking. |
| Archive only the probe's routing anchor between actual requests | Three correct responses, exactly one fallback, following request continued via ordinary route. |
| Real child-process transport | Ordinary EOF and queued 200,004-byte partial/multiple writes correct; cumulative overflow, stdout overflow, cancellation and timeout surfaced; all six owned children reaped. |

Logical reads are Linux `/proc` `rchar`, not physical disk IO. Warm byte measurements follow a cold
observation and do not erase cold-load work. The large-source provider replay and smaller changing
application workload are different measurements: 99.31% is not a promise for every run. Vendor cache
expiry, routing, task changes and genuine generation/reasoning remain relevant. No effort setting
on an installed user run was changed. Local context events separately record unchanged message-prefix
estimates and input-view savings; these are not vendor cache hits or cumulative billing savings.

The command fixture intentionally supplied no project check contract, so its existing qualification
boundary remained open after the requested operations. This verifies real command/checkpoint/recovery
behavior and does not claim qualified-candidate acceptance. These bounded Linux replays are not a
new multi-hour production run or coverage of every platform/provider.

## Failures caught during implementation

1. **Mixed batch loss:** initially, invalidating one read dropped the whole batch, including an
   unaffected completed search. The saved replay now keeps that result and replaces only stale
   read data with an explicit obsolete-source receipt.
2. **Assistant reordering:** an assistant body beginning `ARCHIVED OBSERVATION` was mistaken for
   generated metadata and moved across pending tools. Reordering now applies only to host-produced
   User/Developer metadata outside the pinned prefix; the actual assistant/call/result order passes.
3. **Cold index guard race:** initially, caching the post-load journal size could bless a journal
   changed during the cold scan. The actual coordinated failing replay is preserved. Guards now
   originate at the opened database/binding and are checked again after loading; the replay rejects.
4. **Cache affinity alone was insufficient:** a fresh native cwd/session and regenerated context
   metadata kept breaking the prefix. Empty/unfinished user routing history also synthesized an
   interrupted-turn prefix. A routing-only Developer marker, initialized parent context, native
   prediction fork and stable user-data frame produced the measured large-source reuse.
5. **Early changing host policy:** the first application replay still changed `provider_step` inside
   System. Splitting current Developer state from stable policy removes this avoidable invalidation
   while the exact-current-prerequisite replay checks authority and ordering.
6. **Fallback output-offset panic:** archiving the owned anchor exposed an offset from rejected
   cached-handshake output reused against a shorter ordinary exec stream (`197` versus `77` bytes).
   A guarded pre-inference transport switch now resets only the stream offset, preserves public item
   identities, and bounds-checks observation slices. The exact real archive/retry sequence passes.

## Evidence and replay

[Retained consumers and drivers](reproducers/checkpoint-cache/README.md) exercise the actual public
libraries, TUI, daemon, official account client and operating-system processes. They preserve the
independently defined inputs/invariants and the discovered failures without adding Cargo test targets.

Detailed local artifacts remain outside the repository:

- `/tmp/peritus-checkpoint-oracle-20261019/`: original/current scope comparisons, cold race, migration,
  exact assembled views, context semantics, actual command/restart/rewind and memory journeys.
- `/tmp/peritus-provider-cache-real-20261019/`: actual usage, private official-client wire traces,
  cache-disabled/model-error/anchor-recovery cases and owned-process outcomes.

Build used `CARGO_PROFILE_DEV_DEBUG=0`, `--locked`, `--jobs 2` and the existing external target directory.
The initial behavioral campaign kept installed programs untouched; the subsequent authorized installation is recorded below.


## Authorized local installation

At 2026-10-19 18:19 UTC, the current develop working tree was built with `--locked --release
--jobs 2 -p peritus-cli -p peritus-tui -p peritus-daemon`. A transient build scope capped CPU at
two cores and memory at 4 GiB; compiler temporary files used disk. The optimized build completed
in 5m 51s. All three executables were atomically replaced for both `doll` and `builder`, with
matching source/output hashes, account ownership and mode 0755. Version remains 0.1.2.

- `peritus`: `23a1a10300941d145d8396b50c66c03ed43dff5bab60a66ded900941a95ef6e0`
- `peritus-tui`: `afa86be2eb147a6b8c70d63dbd361de3cb73db19a834ffe9b6746c320b95e406`
- `peritusd`: `7c171dd9f15e4074ede35acd0eb65a928d9f5db9fdc565ecdfb6511f0804e14e`

Installed TUI/daemon reattachment passed for doll's disposable retained run and builder's existing
retained session. Both reported `ready-read-write`; `/proc/<pid>/exe` hashes matched the installed
new daemon. No inference or task operation was submitted. Doll's fixture retained 6 model requests
and 6 tools; builder retained 3,096 requests and 8,838 tools, phase 9 and `user_cancelled=true`.
Builder's new daemon remains ready; the stopped task remains stopped. The disposable doll daemon
was shut down.

During deployment the old builder daemon had already closed its endpoint and removed its instance
marker, while still holding state locks during teardown. The old CLI's `already-stopped` response
therefore preceded process exit. Its retained run was canceled throughout; the old process eventually
exited before replacement/startup. No force kill was performed. The new daemon reopened the state
and session successfully. This is a recorded deployment observation, not a claim that every old
shutdown-teardown timing issue was independently repaired in this change.

Old binaries are backed up in each account's
`.local/state/peritus/binary-backups/checkpoint-cache-20261019T181927Z/`.
Installation manifest and read-only terminal smoke evidence are under
`/home/doll/.cache/peritus-checkpoint-cache-install/`.
