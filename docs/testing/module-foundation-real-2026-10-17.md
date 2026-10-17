# Module foundation: real-interface campaign, October 17, 2026

The delivered foundation supports bundled and separately installed **declarative
workflow packages**, using the same registry, permissions and executor. Source brief
runs gatherer → skeptic → synthesis → final fact checking through real providers.
The larger Deep Research experience remains specified in
[its design](../../.design/deep-research.md): clarification, a visible plan, explicit
Start, structured source-admission decisions and version-bound final approval.
A completed workflow is not a machine-issued guarantee that its prose is true.

## Actual execution

Built native CLI, daemon, gateway and plugin host with one Cargo process at a time,
`--release --jobs 2`; built the actual Svelte assets with `npm run build`. No unit,
mock, formal-verification or repository-verification suites were created or run.
All state, repositories, daemon instances and browser sessions were disposable.
Existing installed binaries and live user jobs were preserved.

| Interface | Independently observed result |
| --- | --- |
| Native CLI registry | **61 operations passed**: bundled discovery, show/enable/disable, install/update disabled by default, corruption detection and exact repair with damaged bytes quarantined, remove, reserved/invalid identifiers, bad manifests, invalid dependencies, symlinks/traversal, preservation after rejected updates, eight concurrent installs/enables without lost updates, and relative/empty XDG fallback. |
| Public module API | **24 generated Unicode/length journeys passed**: ordered stages, concurrent ownership exclusion, declared-dependency denial, exact paging, inspection during ownership, close/reopen, completion and independent artifact corruption. |
| Real terminal/daemon/provider | Official Rust-page retrieval, four distinct role stages and retained outputs, status, completed-job resume with no additional inference. Replayed in an ordinary folder and a real Git-managed worktree. |
| Durable recovery | Commit first stage, interrupt the next, close client, shut down only the owned daemon, reopen the exact run and submit resume once. Original first-stage digest/content remained unchanged; second stage completed. Managed source checkout remained clean. |
| Role isolation | Dedicated per-job/per-stage durable context directories, with six lineages in the managed journey and seven with the browser resource job. The first installed stage returned exactly `RETAINED-FIRST`; the resumed stage returned `RESUMED-FINAL` with that marker. |
| Steering and ordinary Chat | New input during an actual provider request paused the pinned stage without committing its output. Normal Chat handled the pending message, explicit resume completed the original module, and an ordinary Chat reply still worked. |
| Browser UI | Actual Chromium composer and gateway: `/modules`, invalid ID, installed resource-reader inference, exact `café 🦀` resource result, and `/module status`; exactly four human sends, no page errors. Inspected the resulting screenshot. |
| Permission boundary | Installed module declaring `workspace_patch` was rejected before inference; no added model request or new module job. Explicit mode selection remained effective across reattach. |
| Native plugin public API | Actual copied executable plugin, framed initialization/invocation, queued unsent cancellation without poisoning a healthy instance, fresh authorization after admission, stop/start, malformed-frame termination, blocked-pipe cancellation/deadline, simultaneous stop, malformed shutdown acknowledgement with fresh-instance recovery, owned sleeping-descendant cleanup, and UTF-8 diagnostics. Cancel/stop/deadline cleanup stayed below the replay's three-second bound. |

The current native artifact identities and compact receipts are in
[module-foundation-evidence.json](module-foundation-evidence.json).
The [replay guide](reproducers/module-foundation.md) describes the retained executable
consumers and real terminal/browser drivers. Full local raw traces and screenshots
are under `/tmp/peritus-modules-real-20261017/`.

## Confirmed campaign defects, repaired before commit

### MF-01: local command was retried continuously

Initial `/modules` produced a stored reply without consuming the queued input.
The daemon repeatedly retried it: the retained activity sequence reached about
99,500 in roughly 40 seconds, with zero provider requests. Reproduce by submitting
one local module command and watching input incorporation/activity growth.

Fixed with authenticated, durable host-command admission. The immutable request
archive identifies a local command; it does not assert a provider send or reserve
inference usage. Final replays handled each command once and became idle.

### MF-02: stored local replies were invisible

The next replay showed the command incorporated and its result in the run summary,
but no assistant activity in the terminal. Fixed by publishing local results through
the human activity stream, splitting at valid UTF-8 boundaries. Terminal and browser
registry/status/error replies are now visible; large exact output stays in artifacts.

### MF-03: dynamic roles reused ordinary-folder writer history

The generic in-place memory helper mapped unfamiliar role labels to `writer`.
The installed resume fixture read earlier Source brief history, attempted unrelated
`gather`/`fact-check` reads, and returned an inability statement instead of its marker.
The generic managed helper would reject unfamiliar labels rather than provide a
module lineage.

Fixed with a dedicated module-memory constructor binding run, job, stage and provider
role to a separate task/root. Both workspace types now retain isolated stage memories;
no module stage is routed through the coding writer's fallback lineage. Actual marker,
provider, context-directory and restart observations passed after this repair.

### MF-04: first submission after reattach required another Enter

After a real restart, `/module resume` remained in the terminal draft while the
conversation binding loaded. No new input was enqueued. Fixed by retaining one exact
submitted message with its run binding and sending it once after the binding arrives.
Explicit mode travels with that submission. Changed drafts are retained, duplicate
pending submissions are refused, and cancelling the pending binding prevents dispatch.
The real first-submit restart/resume replay passed.

## Additional source-rooted repairs with real execution checks

The older process-plugin host timed only response reads, leaving blocked writes and
queued exchange admission outside cancellation/deadlines. Its cancellation frame
could itself block. It also ignored termination errors, could overwrite stopping
state from a failing invocation, did not serialize simultaneous stop completion,
charged lifetime requests before rejecting concurrency, and truncated diagnostics at
arbitrary UTF-8 byte indices.

The transport now covers admission/write/read, closes the owned process group/job
when a possibly sent exchange fails, reports cleanup failures, and never automatically
replays a possibly effectful invocation. Host admission rechecks lifecycle/authority,
stop is serialized, quota rejection rolls back correctly, and diagnostic cuts respect
character boundaries. The standalone executable consumer exercised these actual
process and public-API paths; it did not substitute a transport implementation.

Installed-package inspection also exposed repair friction: rejecting corrupt bytes
while refusing to reinstall the same identity would leave no usable repair command.
Installation now stages and validates the original package, quarantines damaged bytes,
restores the exact identity and requires explicit enablement again. The CLI replay
checked both restored bytes and the retained corrupt copy.

## Expected refusals and coverage limits

A simultaneous browser attempt while the terminal occupied the same workspace was
refused by the existing workspace-execution guard; the queued input remained durable.
The browser workflow passed after the terminal became idle. Undeclared predecessor
reads and effectful tool declarations are expected denials, not grants of fallback
access.

Execution evidence is Linux x86-64, Chromium, and the signed-in OpenAI account route
(including configured writer/reviewer selection). It does not establish native
Windows/macOS behavior, every provider combination, malicious native-plugin OS
confinement, arbitrary research factual accuracy, or the full Deep Research product
experience. Native process/Wasm plugins remain a separate extension API; the product
module installer never launches package code. No multi-hour module soak is claimed.
