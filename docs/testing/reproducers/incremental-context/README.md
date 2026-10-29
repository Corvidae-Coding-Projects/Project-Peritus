# Incremental context and document pages

These are ordinary consumers of the production public product-tool/context APIs, not Cargo test
targets. The fixture owner independently creates text, mutations and expected outputs. No provider
inference is submitted by the source/context replays; the real configured provider supplies the input envelope.
`plan-live-compare.py` explicitly submits real provider requests through `ProductRunner::converse`.
The separate retained `checkpoint-cache/live-memory.py` journey operates the actual TUI and daemon.

Build the current application using the repository toolchain and a resource allowance appropriate
to the host. The recorded Linux build used `CARGO_PROFILE_DEV_DEBUG=0`, an external
`CARGO_TARGET_DIR`, `cargo build --locked --jobs 2 -p peritus-daemon`, CPU quota 200%, memory high
2500 MiB, hard memory maximum 3 GiB and no additional swap. No unit/formal/repository suites.

Link and operate the freshly built cohort (use an external target directory and fresh output paths):

```bash
python3 docs/testing/reproducers/incremental-context/link.py \
  --deps /path/to/target/debug/deps \
  --output /path/to/reuse-consumer \
  --ordinary-output /path/to/context-consumer
python3 docs/testing/reproducers/incremental-context/replay.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-behavior
python3 docs/testing/reproducers/incremental-context/pressure.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-pressure
python3 docs/testing/reproducers/incremental-context/plan-replay.py \
  --consumer /path/to/reuse-consumer --legacy-consumer /path/to/preserved-older-context-consumer \
  --output /path/to/fresh-plan-behavior
python3 docs/testing/reproducers/incremental-context/plan-live-compare.py \
  --consumer /path/to/context-consumer --output /path/to/fresh-provider-comparison --rounds 2
python3 docs/testing/reproducers/incremental-context/input-invalidation.py \
  --consumer /path/to/reuse-consumer --legacy-consumer /path/to/preserved-older-context-consumer \
  --output /path/to/fresh-input-invalidation
```

`replay.py` uses seed 731944. It drives 96 varied document pages plus line/byte continuation,
UTF-8 errors, mutation and access changes, retention eviction, large-document streaming,
unchanged/extended context, pending tools, current policy and new invocation boundaries.
`pressure.py` observes 48 large sources (enough to exceed decoded retention), revisits an old
source, then changes every source while preserving mtimes and assembles repeatedly. It rejects
loading omitted old bodies on every request or loading obsolete bodies just to make small receipts.

For before/after IO and exact public-output comparison, build the same ordinary context consumer
against the parent revision's production libraries and retain its identity separately:

```bash
python3 docs/testing/reproducers/incremental-context/compare.py \
  --before /path/to/parent-context-consumer \
  --after /path/to/current-context-consumer --output /path/to/fresh-comparison
```

The comparison samples Linux logical read bytes after all API operations, while the child is
blocked writing its large single JSON result. It compares exact page outputs and exact assembled
model messages. Counts include process setup and cold loads, not just warm operations. Wall time
is supporting local evidence, not a portable speed guarantee or vendor cache measurement.

The immediately affected saved replays are `../checkpoint-cache/context-replay.py` and
`../checkpoint-cache/mixed-batch-replay.py`; use the linked ordinary consumer. They cover fresh
process recovery, current permission/source validity, distinct scopes and mixed-batch ordering.

`plan-replay.py` selects the experimental sequential policy in private state. It independently
checks dependent search/read results, exact UTF-8 paging and allocation limits, source changes,
current restrictions, cancelled/stale admission, restart without child redispatch, explicit old
reader refusal of v4, and execution-disabled result retrieval. It contains no provider substitute.

`--decision-view conservative` applies the experimental compiler to those same boundaries.
`--legacy-consumer` takes the ordinary `context-consumer`, whose first argument is the `context`
operation; the line-oriented reuse consumer has a different interface.

The live comparison gives both configurations the same task and source bytes, with a fresh local
state directory each time. A search must identify an unknown source before its eighteenth line
can be read. It verifies exact answers, every source byte, actual tool sequence, all admitted
attempt counts, and per-request usage high-water sums. It records cached usage separately and
alternates the configuration order across pairs. Run without overlapping builds, benchmarks, or
other provider campaigns when interpreting elapsed time. A single pair is exploratory evidence;
it does not establish a latency distribution or justify enabling plans by default.

`input-invalidation.py` changes the actual host conversation revision while the same memory
owner remains open. A conclusion explicitly bound to the old conversation must become stale,
remain exactly retrievable, stay stale when old text returns, and survive a fresh process.
Unchanged source/observation pages retain their identities. An optional older consumer proves
continued v3 readability. `--expect-bug` is for the preserved pre-fix binary and records the
specific defect, not an accepted current behavior.

`decision-replay.py` exercises the opt-in conservative compiler through actual C4 tools and
C6 memory. Its independent oracles check original bytes, same-size/restored-mtime and mode
changes in an undeclared dependency, distinct failed attempts, fixed-snapshot descriptor
paging, current opaque/read restrictions, reviewer namespace isolation, current-state history
rollback and partial raw JSON continuation under a smaller provider envelope. It performs no
inference and establishes no provider-performance result. Add `--require-host-invalidation`
to enforce the R10 oracle for host staleness after an undeclared supplied dependency changes.
The final replay also retains a complete native tool exchange when trailing host feedback is
present. `--expect-native-closure-bug` is only for the preserved prior binary.

`decision-live-compare.py` runs actual ProductRunner chat inference with history and conservative
views. Each linked JSON file reveals the next path; both configurations must read the same
chain in order, return every exact independent value, and leave every byte unchanged. Plans
are disabled to isolate view selection. All actual requests and optional retrievals count;
provider usage is reconciled per request. Fresh local state, reported cache acceptance, binary
identity and configuration order are recorded. Run without overlapping builds/campaigns when
interpreting elapsed time. A regression remains a regression even when the exact outcome passes.

`decision-review-live.py` commits source-backed writer-only C6 scratch, then uses actual review
inference to read the source independently. Public read-only inspection checks the published
review view for exact facts and writer-scratch exclusion. This covers a review conversation;
candidate review and finding conservation require separate journeys.

`candidate-observation-consumer.rs` uses the production managed Git runtime to compare original
repository/source/repository captures with two full captures that each derive both identities.
Link it with `link.py --candidate-output /path/to/new-consumer` after a real application build.
`candidate-observation-replay.py` checks exact identities, independent source/index preservation,
tracked special-node rejection, a real concurrent atomic publisher and alternating warm timing
samples. Git objects/OS caches are warm and native Git traces are enabled symmetrically. Run
without overlapping builds/campaigns for timing; full product/platform qualification is separate.
`candidate-observation-live.py` drives actual managed ProductRunner design/write/gates/review
under both capture policies, requiring exact CSV bytes, current acceptance and unchanged Git
HEAD/index. It performs real inference and no synthetic check/provider substitution.

Use disk-backed storage for larger fixture campaigns. The first campaign encountered the host's
per-user `/tmp` quota despite filesystem free space. Preserve an actual storage failure separately
from behavior/latency failures and rerun the affected operation with writable disposable storage.

For the execution-efficiency increments, use `efficiency.py` rather than the historical
`compare.py` tenfold document-cache target. It checks fixture-owned bytes and every ordered
provider-visible message. To compare checkpoint reuse within one binary:

```bash
python3 docs/testing/reproducers/incremental-context/efficiency.py \
  --before /path/to/current-context-consumer --after /path/to/current-context-consumer \
  --reference-pages --operations 180 --output /path/to/new-page-campaign
python3 docs/testing/reproducers/incremental-context/page-integrity.py \
  --consumer /path/to/current-context-consumer --baseline-consumer /path/to/older-context-consumer \
  --seed /path/to/new-page-campaign/after --output /path/to/new-integrity-campaign
```

The second command uses copies of completed fixture state, checks exact committed dependency
roots, reopens the current checkpoint with reuse enabled, disabled, and an older reader, and
rejects removed, truncated, same-size corrupt, and replaced pages. It never repairs the seed.
`--candidate-first` reverses the comparison order. Alternate order under comparable idle host
conditions and retain repeats; fewer finalizations do not by themselves establish lower latency.

The ordinary consumer's `live` mode can select `conversation_mode` (`chat`, `plan`, `review`, or
`design`) and `provider_config` for a configured production registry with normal OS credential
custody. Live mode consumes inference. All supplied workspaces, trace paths, and command-state
paths must remain disposable. No provider configuration or secret is changed by the consumer.

`cost-ledger-replay.py` qualifies I0 using real inference, paired diagnostics-on/off chat and
review, unavailable-model failure, in-flight cancellation and a managed CSV edit through
acceptance. Run it with a freshly linked ordinary consumer and a new disposable output root:

```bash
python3 docs/testing/reproducers/incremental-context/cost-ledger-replay.py \
  --consumer /path/to/context-consumer --output /path/to/fresh-cost-ledger
```

Its `audit-usage` consumer mode independently decodes canonical C5 provider envelopes and
reconciles per-response high-water usage with accounting. Missing usage stays unknown. The
replay also checks exact answers/source preservation, initial canonical-view equality, absence
of diagnostic sidecars when disabled, exact component partitions and content-free telemetry.
Sampled process-tree RSS can miss short-lived children. Report each local measurement boundary
separately; these cases do not establish a speedup or qualify the later optimizations. See
[the I0 completion report](../../execution-efficiency-i0-cost-baseline.md) for recorded results.

For I1, compare the actual indexed and reference-scan paths in the same freshly built cohort:

```bash
python3 docs/testing/reproducers/incremental-context/indexed-projections.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-index-comparison
python3 docs/testing/reproducers/incremental-context/indexed-projections.py \
  --consumer /path/to/reuse-consumer --ordinary-consumer /path/to/context-consumer \
  --output /path/to/fresh-index-comparison --live-only
python3 docs/testing/reproducers/incremental-context/source-index-trace.py \
  --consumer /path/to/context-consumer --seed /path/to/fresh-index-comparison/live/indexed \
  --output /path/to/fresh-trace-recovery
```

The first command alternates configuration order on large writer histories, then checks the
separate reviewer lineage. Its wire output includes actual tool arguments and exact results.
The second consumes real inference with the installed provider account. The third reopens
copies of its actual native trace and damages only owned C7 records for negative recovery cases.
`PERITUS_CONTEXT_SOURCE_INDEX=reference` is a private host rollback/qualification selection,
captured at memory-owner startup. Unset it to use indexed execution. Neither selection changes
checkpoint formats or authority. Numeric work counters describe the named projection paths;
warm API latency also includes checkpoint publication. See
[the I1 completion report](../../execution-efficiency-i1-indexed-projections.md) for scope,
measurements, regressions and evidence paths.

For I2, operate matched original/reuse owners, trace actual artifact writes separately, and
exercise active-cache corruption and real process interruption on disposable state:

```bash
python3 docs/testing/reproducers/incremental-context/checkpoint-pages.py \
  --consumer /path/to/reuse-consumer --output /path/to/fresh-page-pairs
python3 docs/testing/reproducers/incremental-context/checkpoint-pages.py \
  --consumer /path/to/reuse-consumer --physical --steps 8 --output /path/to/fresh-page-trace
python3 docs/testing/reproducers/incremental-context/checkpoint-pages.py \
  --consumer /path/to/reuse-consumer --warm-corruption \
  --seed /path/to/fresh-page-pairs/reuse --output /path/to/fresh-page-corruption
python3 docs/testing/reproducers/incremental-context/checkpoint-interruption.py \
  --consumer /path/to/reuse-consumer --seed /path/to/fresh-page-pairs/reuse \
  --output /path/to/fresh-page-interruption
python3 docs/testing/reproducers/incremental-context/checkpoint-pages-live.py \
  --consumer /path/to/context-consumer --output /path/to/fresh-page-live
```

The physical run requires `strace` and is excluded from timing measurements. The interruption
run requires GDB/library debug information and temporarily changes only the seed fixture's
source file, restoring it afterward; run it without another campaign using that workspace.
It kills real native processes at artifact/CAS boundaries and uses an actual SQLite publication
failure. The live run consumes inference and requires both managed edits to reach `Accepted`.
`--expect-bug` with `--warm-corruption` preserves the prior matching-byte-symlink defect.
Reuse is enabled by default; `[context.local] reuse_checkpoint_pages = false` retains current
history through the original path. See [the I2 completion report](../../execution-efficiency-i2-checkpoint-pages.md).

## I3 native provider layout

Build the current production cohort, then use `link.py` with `--provider-output` and
`--config-output` in addition to `--ordinary-output`. For example:

```sh
python3 docs/testing/reproducers/incremental-context/link.py \
  --deps target/debug/deps --output /path/to/reuse-consumer \
  --ordinary-output /path/to/context-consumer \
  --provider-output /path/to/layout-consumer --config-output /path/to/layout-config
python3 docs/testing/reproducers/incremental-context/opencode-layout.py \
  --provider-consumer /path/to/layout-consumer --consumer /path/to/context-consumer \
  --config-consumer /path/to/layout-config --key-file /path/to/private-key-file \
  --vendor /path/to/official/opencode --output /path/to/fresh-layout-campaign --rounds 2
```

This consumes real Zen inference through the supported official runtime and selected
`deepseek-v4.1-flash` model. The key is read privately into the normal credential interface;
do not put its value in arguments, environment variables or recorded fixtures. Captures contain
private synthetic prompts and raw native results, not vendor auth/config files. The wrapper
passes native stdout/stderr/status through unchanged and checks native-tool isolation. Fault
generations ask the actual model for malformed envelopes; their altered native instructions
and input are recorded and excluded from savings comparisons.

The direct consumer exercises automatic/required/specific/none/unavailable choices, one/two-call
limits, terminal content, same-model selection, a changed developer marker and bounded real
correction. It compares complete strict schemas with actual host declarations, then executes
admitted calls through real host tools and independently hashes the fixture files. Paired
ProductRunner chats alternate layout order with identical tasks and initial logical views.
Every observed native token partition is reconciled to projections and canonical C5 accounting;
cached input remains part of total input, and unavailable counters remain unknown.

`--resume` rechecks retained completed evidence before continuing; it never resubmits an attempt
with captures but no completed consumer receipt. Final evidence also binds both consumer hashes.
Model compliance with fault instructions is itself observed, not assumed. Preserve an unsuccessful
induction separately before using a fresh case, as documented in
[the I3 report](../../execution-efficiency-i3-provider-layout.md). The prefix experiment reports
local rendered bytes and vendor cache readings separately. Legacy is an explicit route/library
rollback for future requests, with no history migration or automatic replay of an admitted request.

## I4 decision views and host dependencies

Build the production cohort and relink both consumers as above. Use fresh output directories:

```sh
python3 docs/testing/reproducers/incremental-context/decision-replay.py \
  --consumer /path/to/reuse-consumer --output /path/to/compiler --require-host-invalidation
python3 docs/testing/reproducers/incremental-context/decision-host-dependencies.py \
  --consumer /path/to/reuse-consumer --old-consumer /path/to/preserved-phase-four-consumer \
  --output /path/to/dependencies
python3 docs/testing/reproducers/incremental-context/decision-ignored-input.py \
  --consumer /path/to/reuse-consumer --output /path/to/ignored-input
python3 docs/testing/reproducers/incremental-context/decision-resolved-progress.py \
  --consumer /path/to/reuse-consumer --output /path/to/resolved-progress
python3 docs/testing/reproducers/incremental-context/decision-host-update-interruption.py \
  --consumer /path/to/reuse-consumer --output /path/to/update-interruption
python3 docs/testing/reproducers/incremental-context/decision-v5-upgrade.py \
  --consumer /path/to/reuse-consumer --v5-consumer /path/to/preserved-phase-four-consumer \
  --output /path/to/compatibility
```

The kill campaigns require GDB and current library debug information. They stop only their owned
consumer processes. The prior phase-four consumer must really support checkpoint v5; a reader
that only supports v4 does not prove the new v6 floor fences the unqualified v5 semantics.
Committed roots are checked through SQLite publication history, including complete artifact
dependencies. Unpublished artifacts left by failed publication are not committed roots.

Actual-provider continuation, review, managed edits and paired comparisons consume inference:

```sh
python3 docs/testing/reproducers/incremental-context/decision-continuation-live.py \
  --consumer /path/to/context-consumer --output /path/to/continuation
python3 docs/testing/reproducers/incremental-context/decision-review-live.py \
  --consumer /path/to/context-consumer --reuse-consumer /path/to/reuse-consumer \
  --daemon target/debug/peritusd --output /path/to/review
python3 docs/testing/reproducers/incremental-context/candidate-observation-live.py \
  --consumer /path/to/context-consumer --candidate-policy separate --output /path/to/managed
python3 docs/testing/reproducers/incremental-context/decision-live-compare.py \
  --consumer /path/to/context-consumer --opencode-key-file /path/to/private-key-file \
  --model deepseek-v4.1-flash --repeat-list --nodes 8 --rounds 2 --output /path/to/inventory
```

The last command explicitly selects the official OpenCode runtime and Zen `deepseek-v4.1-flash`; omit
`--repeat-list` for the simple chain control. Other live drivers use the configured native Codex
route. Without `--model`, the comparison driver selects `gpt-6.1-sol`, including through Zen;
keep different model cohorts separate. Preserve incomplete provider attempts separately; never
resume them as passing samples.
`--resume` on completed comparisons verifies consumer identity and exact existing fixture bytes.
Full input includes cached input, and naturally different request/correction counts stay visible.

`local_context.decision_view = "conservative"` is opt-in. `"history"` remains the default and
rollback from current committed state. Both retain full tools, exact source paging, current
authority and qualification gates. Host semantics require a committed checkpoint-v6 guard;
rollback keeps that floor, sticky staleness and declared completed progress. See
[the I4 completion report](../../execution-efficiency-i4-decision-views.md) for measured scope,
saved regressions, exact versions and limitations. I6 qualification is recorded separately below.

## I5 sequential plans, waits and live control

Build the production cohort and link the actual public consumers, including the IPC and
native-owner consumers:

```sh
python3 docs/testing/reproducers/incremental-context/link.py --deps target/debug/deps \
  --output /path/to/reuse-consumer --ordinary-output /path/to/context-consumer \
  --control-output /path/to/control-client --command-output /path/to/command-owner
python3 docs/testing/reproducers/incremental-context/plan-replay.py \
  --consumer /path/to/reuse-consumer --output /path/to/plan-replay \
  --legacy-consumer /path/to/preserved-v3-context-consumer
python3 docs/testing/reproducers/incremental-context/plan-interruption.py \
  --consumer /path/to/reuse-consumer --output /path/to/plan-interruption
python3 docs/testing/reproducers/incremental-context/command-owner-recovery.py \
  --consumer /path/to/command-owner --output /path/to/owner-recovery
```

The plan replay can additionally use `--decision-view conservative`. Real process kills and
publication failures operate only on owned state. Uncommitted finalized artifacts must not be
counted as committed observations. Fresh-process retrieval and source descriptors are independent
of plan enablement. The original native owner is reconciled without another launch; an
indeterminate outcome must remain indeterminate.

The following drivers consume real inference. They require GDB/current library debug information
and official credential-owning Codex. Use fresh output roots and the actual built `peritusd`:

```sh
python3 docs/testing/reproducers/incremental-context/plan-daemon-control.py \
  --daemon target/debug/peritusd --cli target/debug/peritus --client /path/to/control-client \
  --output /path/to/daemon-control
python3 docs/testing/reproducers/incremental-context/plan-daemon-control.py \
  --daemon target/debug/peritusd --cli target/debug/peritus --client /path/to/control-client \
  --cases review-pause plan-crash wait-permission-restore plan-before-edit \
  --output /path/to/role-restart-authority
python3 docs/testing/reproducers/incremental-context/command-wait-live.py \
  --consumer /path/to/context-consumer --output /path/to/interactive
python3 docs/testing/reproducers/incremental-context/plan-live-compare.py \
  --consumer /path/to/context-consumer --model gpt-6.1-sol --rounds 2 \
  --output /path/to/codex-plan-comparison
python3 docs/testing/reproducers/incremental-context/plan-live-compare.py \
  --consumer /path/to/context-consumer --model deepseek-v4.1-flash \
  --opencode-key-file /path/to/private-key-file --rounds 1 --output /path/to/zen-plan-comparison
```

The IPC client explicitly confirms the objective before starting a goal, and queries actual
workspace permissions rather than changing a fixture flag. Non-stop debugger gates release all
other daemon threads so real IPC can steer the run. Each wait control checks that the original
process is still alive. The interactive driver resumes the host before independently releasing
the native process, so debugger suspension is not substituted for active waiting.

Paired tasks keep the same exact source bytes and task/model settings, alternate order for
multiple pairs, and independently reconcile native C5 usage. Keep incomplete or rejected plan
attempts separately. A literal and a result reference cannot supply the same argument; omit the
referenced field entirely, including placeholders. Direct tools remain available.

Plans remain opt-in with `[context.local] read_only_plans = "sequential"`; `"disabled"` keeps
current guarded history and exact result retrieval. No rollback downgrades the reader or
restarts an uncertain command. See [the I5 report](../../execution-efficiency-i5-plans-waits.md)
for exact cohorts, saved regressions, measured benefit and coverage limits.

## I6 full candidate observations

Build the actual daemon/CLI/TUI cohort, then link both the ordinary ProductRunner and full
capture consumers. Use fresh disposable output paths. Native `live` and `resume` commands
consume actual inference; the other two commands use real local APIs/Git without inference.

```bash
python3 docs/testing/reproducers/incremental-context/link.py --deps target/debug/deps \
  --output /path/to/reuse-consumer --ordinary-output /path/to/context-consumer \
  --candidate-output /path/to/candidate-consumer
python3 docs/testing/reproducers/incremental-context/candidate-observation-replay.py \
  --consumer /path/to/candidate-consumer --output /path/to/new-freshness --rounds 12
python3 docs/testing/reproducers/incremental-context/candidate-observation-interruption.py \
  --consumer /path/to/candidate-consumer --output /path/to/new-capture-interruption
python3 docs/testing/reproducers/incremental-context/candidate-observation-live.py \
  --consumer /path/to/context-consumer --output /path/to/new-managed
python3 docs/testing/reproducers/incremental-context/candidate-observation-live.py \
  --consumer /path/to/context-consumer --candidate-policy shared --workspace-kind in-place \
  --output /path/to/new-in-place
python3 docs/testing/reproducers/incremental-context/candidate-observation-resume.py \
  --consumer /path/to/context-consumer --output /path/to/new-candidate-resume
```

Run the warm paired measurement without overlapping builds or native journeys. It checks
independent source/index preservation, exact identity correspondence, a real concurrent writer,
linked worktrees, unsupported kinds and actual Git object-publication refusal/recovery. The
interruption driver uses GDB against production capture boundaries and checks fresh-process
reacquisition. The resume driver deliberately interrupts a real writer before gates, then
continues its actual durable state after source/HEAD changes or switching to Separate capture.
`--cases` can select a named resume journey; it does not bypass its independent acceptance.

Set `local_context.candidate_observation = "shared"` for opt-in managed Linux capture sharing.
`"separate"` is the default and independent rollback from current state. Other platforms always
use separate captures; in-place task scopes retain their existing path. Keep serial plans and
complete catalogs. See [the I6 completion report](../../execution-efficiency-i6-freshness.md)
for exact cohorts, timings, platform limits and the retained setup/oracle corrections.


## v0.1.14 default-policy replay

`link.py --defaults-output <binary>` also links `context-defaults-consumer.rs` against the
fresh daemon/product libraries. `context-defaults.py --consumer <binary> --output <fresh-dir>`
checks seven independent daemon TOML and partial JSON cases, including both explicit opt-outs.
`decision-replay.py`, `plan-replay.py`, `plan-daemon-control.py`, `plan-live-compare.py` and
`candidate-observation-live.py` accept `--use-defaults` to omit the two optimization settings.
The live plan replay requires an actual model-authored dependent plan and observed decision-view
compilation; an ordinary successful answer alone does not qualify the default route.
