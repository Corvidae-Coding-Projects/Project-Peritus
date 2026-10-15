# In-place directory and memory recovery: real testing, 2026-10-13

This fixes the live browser run's repeated `declare exact task files, not a nonempty directory`
failure. A command could populate a directory that Peritus had already admitted while empty;
later tool checkpoints and finalization then rejected that same scope. The problem was in
durable host tracking, not in the requested browser operation or provider authentication.

No unit/mock tests, formal verification, lint gates or qualification suites were created or
run. The actual CLI/daemon were built with `cargo build --locked -p peritus-cli -p
peritus-daemon --jobs 2`. The personal `/home/doll/browser` workspace and installed daemon
were left untouched. Source is develop at base `c609e038c995aeaea14255703f31c70fef28d73a`
plus the working changes. Evidence is under `/home/doll/Peritus-OpenCode-2026-10-13/`.

## Corrections

Declared directories now retain an original bounded subtree baseline and discover admissible
new descendants for later comparison. Empty, absent and legacy-v1 declarations stay valid
after population. Overlapping declarations preserve the first baseline. Command effects on
new outputs count toward the observed candidate; new comparison evidence is not falsely
represented as an automatic before-image checkpoint.

Subtree admission bounds apply before new work, not after already admitted commands create
more files. Directory/file replacement treats obstructed old descendants as absent. Later
symlink, socket and FIFO transitions produce metadata evidence without following or opening
the node. Initial explicit symlinks remain rejected. Implicit inventory continues to omit
protected paths and conventional caches. An explicitly requested cache file remains an
authorized exact inspection. Repeated identical settlement diagnostic tails are deduplicated.

## Independent public-library replay

The independent oracle was written before the replay, separately from the implementation
diagnosis. A minimal consumer invokes the production public ScopedBaseline API in fresh
processes. Real Python processes, listening Unix sockets, FIFOs, GNU patch and filesystem
readbacks supply the effects. No copied implementation or mock service substitutes for it.

The final independent run and the retained configurable entrypoint each passed all 20
journeys. Each used 122 fresh consumer invocations, 65 filesystem mutation processes, two
GNU patch processes and two listening Unix-socket children; both socket children exited zero.
Every operation had a five-second bound. Longest consumer calls were 0.120/0.121 seconds.
No unrelated copied source changed, and external old-atime canaries remained unread.

The fixtures archive 286 actual Peritus source/doc files. Cases cover nonempty brownfield
edits, create/modify/delete/reopen, empty and absent output growth, v1 recovery, overlapping
file/parent declarations, directory deletion, both file/directory transitions, protected and
implicit cache/link exclusion, and varied plain/space/Unicode/nested/long names with seed
260213. Additional actual command growth created 4,105 children and 70 nested levels after
admission. Opaque-node cases cover socket and FIFO lifecycles, file and nonempty-directory
replacement by external symlinks with matching child names, initial-link rejection, and a
mixed opaque-node parent inventory.

An initially failing directory-to-file case reproduced ENOTDIR in two fresh processes.
After the fix, the same concrete retained descriptor reports the root kind change and old
descendant deletions. The initial broad cache-file rejection expectation was a driver oracle
error: explicit cache inspection was already allowed. Correcting that expectation preserved
all implicit-inventory no-read canaries and added the positive exact-cache journey.

Final evidence:

- `independent-scope-replay/report.md`
- `independent-scope-replay/runs/20261013-172718/{results,campaign-summary}.json`
- `independent-scope-replay/retained-opaque-final/runs/20261013-172806/{results,campaign-summary}.json`
- Original ENOTDIR reproduction: `independent-scope-replay/runs/20261013-164229/directory-to-file/`

The retained [consumer and replay instructions](reproducers/in-place-scope/README.md) operate
the actual library using configurable checkout, executable and disposable output paths.

## Real client and daemon

Two isolated source copies were operated through the native terminal client with real
providers. Each declared the empty `repros/scope-live/outputs` directory, launched the actual
Python producer, then patched and read back the generated marker and nested JSON. The
expected marker is `verified-after-cancel\n`; the independently parsed JSON value is 46.

The Codex account run `9bcdb4efa8ce86c65bf46693aabd238a` called command_cancel and
command_recover on the existing owned handle, patched the marker, read the artifacts, and
reached `Idle — unqualified changes retained` with no interruption cause. The producer had
already exited zero; this establishes recovery of that terminal handle, not termination of a
still-running process. The code remains explicitly unqualified, with no fabricated gate pass.

The Zen/OpenCode run `e59650fd75e0f6412b0b083eda244c4b` also performed the real producer,
polling, patch and both readbacks. It honestly reported natural exit rather than claiming
cancellation. `scope-native-final-fixture/source-preservation.json` records the expected
outputs and identical hashes for all 5,706 unrelated copied files. Its final handoff evidence
did not complete: that fixture retained an invalid omit-purpose test instruction and repeated
review reads. Its original failure evidence stays retained; user acceptance or commit was not
simulated. The fixture instruction was corrected because the declared command schema requires
`purpose`, rather than changing the directory/output acceptance criteria.

The final native client run `115020ffb374d5827d884d1bee7e082b` in
`scope-native-literal-fixture` completed the actual producer, stdout read while active,
cancellation and terminal recovery (signal 15), marker patch and both readbacks. Its independent
review returned no findings and status `Completed in place — tracked task files passed checks
and independent review`. The only automatic exact-target check recorded was JSON structure;
the user-excluded suites and build gates were not run. The final resumed observation took
176.47 seconds; earlier failed prompt/cwd attempts remain in its original cumulative trace.

A fresh Codex client run `d582129443f4123bb3afc0d18ca555c0` also patched/read the outputs and,
after restarting the disposable daemon and retrying its saved record, reached
`Idle — unqualified changes retained`. It did not fabricate qualification or independent
review. Its final resumed observation took 54.27 seconds. Both copies independently retained
identical hashes for all 5,706 unrelated source/doc files (`source-preservation.json`).
The original four-minute observer timeout is preserved separately and is not a handoff pass.

## Confirmed active command-output ownership defect

The additional application replays exposed `command_output requires a handle owned by this
run` for a handle the same run had just started and polled. Output lookup consulted only the
recovered owner map; newly dispatched active/terminal owners were stored on disk without
entering that in-memory map. Codex and OpenCode both encountered the defect.

The runtime now retains exact owners for newly accepted active and completed dispatches,
using the same owner table for output retrieval and restart recovery. Active lifecycle controls
still use their original router attachment; registering an owner does not divert live
cancellation into a recovered-process path. The final native run successfully read stdout
while running, observed `fixture-created`, cancelled that exact handle, and recovered its
terminal observation before editing the command-generated files. Unknown and cross-run
ownership checks remain in place. The pre-fix records are retained in the ordered fixtures'
`before-active-owner-fix.json` and terminal captures.

## Confirmed memory feedback defect

The first longer Zen application replay stalled before patching the marker. Its archive
contains repeated context_read outputs whose targets were earlier retrieval receipts,
recursively nesting escaped JSON. Three host paths encouraged this: omission notices,
automatic older-evidence/compactor selection, and generic source-handle annotations on the
retrieval receipts themselves. The original raw observations remain retained in
`scope-live-fixture` for audit.

Retrieval/working-state/review-evidence receipts now stay in the exact archive without being advertised as
new task evidence by those automatic paths. A context_read result already carries original
source handles and byte offsets. Its generic receipt annotation is omitted, and omission
notices describe optional references instead of instructing the model to reread every result.
Those older references and optional previous-invocation assistant previews precede the current
exchanges, preserving the newest tool results as
the next turn's immediate context. The first native review replay exposed repeated evidence
reads when the old archive notice was appended as a new user turn after them; its snapshot
is retained in `scope-native-final-fixture/reviewer-loop-before-ordering-fix.json`.
Explicit audit retrieval remains available; this introduces no operation budget or read gate.

The fresh final Zen writer reached the requested patch/readback after six context_read calls,
with zero reads of retrieval receipts. This early writer result is not the final full-run
acceptance result described above. The actual archive-derived count is retained in
`scope-native-final-fixture/memory-audit.json`. Codex and Claude separately passed real
provider/tool-result filesystem exchanges on the changed code; the shared memory correction
is intentional and does not change their provider adapters.

## Coverage limits

These are bounded filesystem, public-library, provider and client/daemon journeys, not
exhaustive coverage or a multi-hour campaign. Socket/FIFO execution was Linux-only. Resource
admission can still reject a new excessively broad scope before its effects, and genuine
permission, disk or journal corruption errors remain visible. Automatic restoration of every
newly discovered command output was not promised or exercised. Other-provider and native
isolation results are recorded in the [OpenCode campaign](opencode-runtime-real-testing-2026-10-13.md).
