# Incremental execution efficiency: implementation and evidence

Objective: complete all implementation stages I0 through I6 in
[the design](incremental-execution-efficiency.md). The scope includes all 16 requirements and
all 15 acceptance journeys; partial implementation or a successful build is not completion.

Status: **I0–I6 are complete for their increment contracts: seven of seven phases.**
The latest instruction authorized I6 (phase seven). See the closing audit and bounded coverage
below; historical campaign entries retain the status they had when recorded.

## Baseline

- Starting branch: `develop`.
- Starting HEAD: `d58ccbed426fba0a361a6533bb4ddb80726ba2ce`.
- Original design SHA-256: `5cb00c34f2fb568636d37dd913580a4180f70d14ba675e955d752a7e256e0915`.
- Current design SHA-256: `e540f7bdaf5c60159dfd958d8b28d471ef39bf2b629540f6b706a4798c029b43` (qualified I4 v6 dependency/progress guard and I5 plan/wait permission-revision/reference contract and I6 delivery boundary; requirements retained).
- The worktree contains preexisting navigation, CLI, computer-tool, lockfile, and documentation
  edits. They are retained; this work does not reset, stage, commit, or discard them.
- The first actual baseline build uses the pinned Rust 1.97.1 toolchain and
  `cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui`.
- Existing real public-API consumers under `docs/testing/reproducers/checkpoint-cache` and
  `incremental-context` will be inspected and linked against the actual application cohort.
  Any stale consumer interfaces must be corrected before their outputs count as evidence.
- No active `.crosslink` directory exists. Retained `.chainlink` state is not migrated or
  initialized by this implementation.

## Stage ledger

| Stage | Implementation | Qualification | Evidence and scope |
| --- | --- | --- | --- |
| I0: Cost baseline | Role/reason/attempt/terminal accounting, rendered component byte/character partitions, native correction usage, local/tool timing and counter ledger, and true diagnostics-off implemented | Complete | Seven real current-build cases pass with independent canonical C5 accounting, exact outcomes, on/off view equality and failure/cancellation coverage; see the completion report below |
| I1: Indexed projections | Existing checked source lookup reused for cached ranking; suffix ingestion, tool maps and archive totals implemented, with faithful reference rollback and numeric counters | Complete | Three current-build pairs preserve exact ordered text/tool views, arguments and rejections; 318 actions and 3,096 source deliveries checked; native continuation, trace identity recovery, integrity and varied replays pass; see I1 completion report |
| I2: Immutable page reuse | Verified lineage/range/page-identity reuse enabled by default; regular-object verifier fixed; encoding/write counters and explicit rollback retained | Complete | Exact components/views and all 190 roots pass; measured artifact writes fall 92.8%; 32 alternating pairs improve local median 31.2%; physical traces, 14 warm corruption, 14 interruption/publication, eight cold/older-reader cases and two real accepted edits pass; see I2 report |
| I3: Provider layout | Once-only full-schema delivery enabled by default; explicit legacy rollback survives model selection; total input includes cache partitions | Complete | 25 real C5/ProductRunner cases, 31 native inferences and nine route cases pass; paired total input falls 31.7%, latency is mixed; see I3 report |
| I4: Decision-view compiler | Qualified opt-in exact compiler; durable originating dependencies, complete native suffix and declared progress under v6; history default/rollback retained | Complete | Binding/closure/completed-plan regressions, migration/publication/kills/restart, paired real inference, changed-source continuation, independent review and Accepted managed edit pass; measured gains vary by workload; see I4 report |
| I5: Read-only plans and waits | Qualified sequential plans, exact results/continuations, v4 guard, host role/counters and owned waits; durable permission revision and complete reference guidance; plans opt-in | Complete | Real daemon steering/revocation/pause/cancellation, reviewer accounting, interactive input/output, plan/command publication kills and full daemon restart pass; final Codex/Zen input falls 29.6%/24.5% with two requests instead of three; see I5 report |
| I6: Targeted freshness work | Qualified opt-in managed Linux shared full captures; original default/rollback and conservative non-Linux/in-place paths retained; no format migration | Complete | Eighteen equivalence groups, concurrent writer, linked worktree, publication refusal, native kills/reacquisition, six Accepted managed/direct/continuation journeys and 46 actual responses pass; twelve warm pairs reduce median local capture time 31.1%; see I6 report |

## Completion audit

The increment reports and closing traceability table below record the implementation and real
journeys for R01–R16 and A01–A15. All seven increments are complete within their stated delivery
boundaries. Experimental paths remain opt-in where general benefit is not established;
unsupported platforms retain the original implementation. Unexercised providers/platforms and
optional future experiments are not counted as passing qualifications.

## Observed campaign results

Disposable evidence root: `/home/doll/.cache/peritus-efficiency-dif0njrm`. Baseline public consumers
are static binaries linked before production edits. Binary hashes and replay inputs are retained
with each result. No inference has been submitted by these local-context replays.

- Baseline real application build passed with the command above. Baseline context replay passed
  six source/recovery/permission/isolation/prefix cases (`baseline/context-replay/result.json`).
- I1 comparison (`candidate/i1-exact/result.json`): 24 operations, 264 independently checked
  source deliveries per cohort, exactly equal ordered provider-visible messages. One sample
  measured 2.641 seconds before and 2.670 after; this does not establish a speed improvement.
- I2 same-binary flag comparison (`candidate/i2-pages/result.json`): 180 operations, 2,760
  independently checked source deliveries per configuration, exactly equal ordered messages.
  Both published 359 checkpoints. Finalizations decreased 3,993 to 1,803; completed artifact-write
  bytes decreased 59,551,882 to 28,090,646. Full verification still hashed 31,462,786 reused bytes.
  Process elapsed time increased 38.656 to 65.991 seconds in this single ordered sample. Reuse
  remains disabled by default pending explanation and repeated qualification.
- The retained historical `incremental-context/compare.py` replay stopped at its preexisting
  tenfold document-read reduction assertion, which compares an older document-cache feature.
  Both cohorts already include that feature. Its oracle was preserved. The new `efficiency.py`
  comparison independently checks fixture bytes and exact views without that unrelated target.
- I2 reverse-order repeat (`candidate/i2-pages-reverse/result.json`) again preserved every view
  and the same structural write reductions. Elapsed times were 30.700 seconds with reuse and
  77.712 without. Linking/another correctness replay overlapped parts of this repeat; it is
  explicitly unsuitable for a controlled latency claim. The two samples show why elapsed
  performance must be remeasured under comparable conditions before enabling reuse by default.
- Current-page integrity campaign (`candidate/i2-page-legacy/result.json`) passed eight checks:
  exact current checkpoint dependency roots, fresh-process reuse, reuse-disabled rollback,
  opening the new checkpoint with the preserved baseline binary, and refusal of removed,
  truncated, same-size/restored-mtime corrupt, and replaced working pages. Failed recoveries
  did not change the committed root. The fixture's first attempt used the wrong object-path
  suffix; the path was corrected against the actual store layout before these results counted.
- Varied public tool/context replay (`candidate/i1-varied/result.json`) passed nine case groups,
  including 96 independent document pages, UTF-8/continuation errors, same-size content changes,
  modes/symlinks, permissions, cancellation, retention eviction, current host policy/input,
  pending-to-settled tool ordering, and invocation rollover. This is bounded functional coverage,
  not a whole-codebase or all-provider claim.
- Actual Codex ProductRunner chat (`candidate/i0-live/result.json`): model `gpt-6.1-sol`, existing
  high-effort runtime mapping, two real file reads, two inference requests, exact independent
  fixture values in the final reply, 30,450 reported input tokens, zero reported cached input,
  and 217 output tokens. Request diagnostics matched the admitted request count; independent
  per-request high-water sums matched cumulative accounting. Other optional counters remained
  null. Prefix measurements described rendered parts, not cache acceptance. This is one
  successful real interface journey, not a speedup comparison.
- The retained OpenCode route could not reach inference: its old Codex executable path no longer
  existed, then, after changing only the disposable configuration copy to the installed path,
  its saved Zen credential was missing. Original configuration and credential state were not
  changed. This is an explicit qualification gap; once-only layout remains opt-in.
- The affected production crates passed strict Clippy for all their targets/features with two
  workers (`candidate/clippy-affected-r4.log`). New lint findings were fixed at their source,
  including extracting the measured-attempt boundary and prepared-view validation. The complete
  workspace format check passed before those final extractions; repeat it on the final state.

The provider and large-history results above identify their own binary hashes. Later refactoring
and diagnostics changes require a fresh build/relink and the affected replays before any stage
can be called qualified for the final source state.

### Checked I0–I3 cohort

`candidate/checked-source-identity.json` and `candidate/checked-binaries.json` retain exact source
and consumer identities after the Clippy refactoring. That cohort passed `checked-exact`,
`checked-page-integrity`, and `checked-varied`: exact before/after views, eight integrity/rollback
checks, and the nine varied case groups (264 public API actions). Its fresh actual Codex chat
also returned newly generated fixture values exactly and reconciled two requests with 30,448
input tokens and 223 output tokens (`checked-live/result.json`).

A real unsupported-model request produced one admitted request, no host tools, no reply, a
`model_unavailable` failure and no observed usage (`checked-failure/result.json`). Missing usage
was explicitly unknown. This exposed a diagnostic ambiguity: driving a correctly framed failure
stream can succeed while the provider request failed. The subsequent instrumentation adds an
explicit normalized terminal outcome alongside `drive_failed`. That last diagnostic change
passes affected all-target/all-feature strict Clippy (`clippy-terminal-outcomes.log`). Its own
build/relink and real negative replay also passed (`terminal-failure/result.json`): one admitted
request, zero host tools, normalized outcome `failed`, correctly framed stream closure, no reply,
and unknown usage. `fmt-terminal-check.log` is empty and successful. These diagnostic checks do
not qualify the still-missing broader campaigns or implementation stages.

`i0-diagnostics-off/result.json` confirms exact views with diagnostics disabled and no new
efficiency sidecar. `fmt-final-check.log` was empty and successful. No tests or working-state
copies here establish the still-missing interruption-at-publication or I4–I6 journeys.

## Remaining implementation boundaries

- I4 must extend the existing working/run-knowledge machinery and distinguish exact host source
  versions from model-authored conclusions. Preserve candidate-wide/unknown dependencies and
  independent reviewer isolation. A reusable request key must cover current input/control,
  permissions, source versions, profile/capabilities, tools/order, policy and compiler revision.
  Merely adding a focus label or another arbitrary prompt fraction is not completion.
- I5's implemented source-policy recursion, v4 reader guard, original parent bindings, explicit
  actual role, and forwarded host-child accounting must remain intact. The per-child live
  reservation/completion and passive `tool_control` path still need real daemon steering/pause
  qualification; compile success and fixture-only cancellation are insufficient.
- Owned waits retain the original handle and release owner locks between polls. Complete
  interactive output/input, permission/steering/pause during a live wait, uncertain outcomes,
  process death, and interrupted terminal-publication journeys before qualifying I5.
- I6 remains conditional on demonstrated equivalent freshness and net benefit. Current full
  candidate checks, conservative scans, serial reads and complete catalogs remain required
  fallbacks until an experiment proves a specific replacement.

## I5 implementation and bounded evidence

The current opt-in `read_only_plans="sequential"` contract supports closed version-1 node graphs,
earlier-result JSON Pointers, ordinary workspace list/search/read execution, exact child JSON
artifacts, response-size continuations, and source-policy-checked result paging. Every child
uses fresh ordinary admission and the actual host model role; children are distinct host
records rather than provider-native calls. The host durably publishes a v4 reader guard before
child evidence. Current readers support 1–4, default runs remain v3, older binaries refuse v4,
and disabling execution retains v4 result retrieval. The design records this format amendment.

`command_wait` operates an existing C4/C2 handle. It waits for terminal state or selected exact
stream output, checks cancellation/current input/permissions/passive goal control, and ends only
observation when yielded or explicitly time-limited. Original process ownership, stdin and
recovery tools remain. Passive control observations do not reserve or complete additional calls.

- `i5-final-plan-replay/result.json` passes nine independent case groups: inert graph/mutation
  rejection before upgrade, dependent search/read exact facts, no-match judgment, aggregate
  size limits and exact UTF-8 pages, fresh-process continuation without child redispatch,
  same-size changed-source bytes, cancelled/stale admission, opaque/read restrictions and
  foreign scope, older-reader refusal without root replacement, and disabled-execution retrieval.
  The first replay incorrectly treated a live leave-alone constraint as a read ban. Its
  failure is retained in `i5-plan-replay`; the fixture now uses an actual opaque-interface
  read restriction rather than changing a construction-only private scope in place.
- Actual Codex `gpt-6.1-sol` chat (`i5-plan-live/result.json`) returned three fresh independent
  values exactly through one provider-proposed plan and three recorded child reads. Two
  inference requests reported 32,462 input and 389 output tokens. This is functional evidence,
  not a comparison against direct independent reads.
- The final-cohort dependent search→read comparison contains four pairs, one in reversed order,
  with identical relative source bytes/task/model settings per pair and fresh private local
  state. All eight answers match independent fixture values; every source byte remains unchanged.
  Direct tools used three requests per task; plans used two. Total input was 183,285 versus
  128,616 tokens (29.8% lower); all eight reported zero cached input. Output was 1,119 versus
  1,433 tokens (higher with plans). Median elapsed time was 24.731 versus 19.731 seconds; mean
  was 29.719 versus 22.206. One plan case took 31.807 seconds against its paired direct case's
  20.980. This small single-workload Linux campaign does not establish universal improvement.
  Plans remain disabled by default. Raw cases, exact usage high-water reconciliation and
  binary identity are in `i5-dependent-live-comparison`, `i5-dependent-live-repeats`, and
  `i5-dependent-live-summary.json`.
- Actual `command_start`→`command_wait` (`i5-active-wait-live/result.json`) spent 50.964 seconds
  in the host wait, returned the exact exit-0 output from the original handle, issued no model
  `command_poll`, and left exactly one independent start marker. A fresh consumer reconnected
  the same native owner and read the exact successful terminal output without another start.
  The complete product run ended `CandidateAvailable`: its fixture had no applicable exact-target
  check contract. This correctly preserved the existing acceptance boundary; it is a wait-interface
  pass and explicitly not a qualified end-to-end product success. The earlier 3-second fixture
  (`i5-wait-live`) had already exited before the model returned its handle and only verifies
  terminal retrieval, not active waiting.
- The final source cohort builds daemon, CLI and TUI with two Cargo workers, passes affected
  all-target/all-feature strict Clippy (`i5-final-clippy.log`), full workspace formatting
  (`i5-final-fmt.log`), the nine varied context groups (`i5-final-varied/result.json`), and the
  eight v3 page integrity/recovery/rollback checks (`i5-final-page-integrity/result.json`).
  `i5-final-source-identity.json` and `i5-final-binaries.json` identify this cohort. Earlier
  live plan/wait samples retain their own pre-final-cohort binary hashes; the four paired
  dependent-read comparisons use the final cohort.

I4, I5 and I6 are not qualified complete; the full active objective and
all-stage acceptance audit remain open. No repository commit, push, release, or live-state
migration has been performed.

## I4 input-invalidation foundation

Tracing the compiler prerequisites exposed an actual live-input defect. The context domain
already supports conversation rebinding, but local memory reused the conversation revision
captured at open. With one real memory owner kept alive, a new literal host input reached the
assembled request while a conclusion explicitly bound to the old conversation stayed `open`.
`i4-input-before/result.json` records that failure against a preserved pre-fix binary.

Refresh now captures the current fallible interaction input revision (or the embedding's live
conversation revision), commits the existing C6 refresh event, and keeps the host binding aligned
with the committed state. Recovery finishes old observations under their recovered epoch before
admitting current input. It does not relabel old evidence as newly observed. A backward revision
is refused. The existing page cache can carry identical immutable observation/source pages
across a same-lineage conversation rebind, still requiring exact current-state page identity and
full artifact verification. Page/event codecs and v3 checkpoint compatibility remain unchanged.

`i4-input-legacy/result.json` verifies removal of the old conclusion from the current view,
exact retained `stale` retrieval, sticky invalidation, fresh-process recovery, unchanged page
identities, and continuation with the preserved older reader. The same source cohort passes
the nine plan-boundary groups (`i4-input-plan-regression`) and nine varied context groups
(`i4-input-varied`). Its application build and strict affected Clippy logs are
`i4-input-build.log` and `i4-input-clippy.log`; `i4-input-fmt.log` records formatting.
Source/binary identities are in `i4-input-source-identity.json` and `i4-input-binaries.json`.
The conversation/module/native wrappers also explicitly forward host-child accounting.

This is a correctness prerequisite, not a completed decision compiler or a measured speedup.
Model-authored dependency narrowing, exact needed-evidence selection, reconstruction keys,
independent reviewer isolation, and the corresponding real-provider campaigns still require work.

## Verification discipline

Use disposable workspaces and state roots. Keep builds at two Cargo workers and one build at a
time. Exercise the real application/public library consumers and retain independently specified
expected results. Do not manufacture provider results, substitute mocks, or add unit/formal
suites. Preserve actual failures and replay inputs; report the exact final artifact exercised.

Restore optimizations from current committed state when comparing paths. Never restore an old
production state directory, erase evidence, weaken integrity checks, or redispatch an uncertain
effect to make an efficiency measurement look better.


## I4 conservative compiler implementation and bounded evidence

`DecisionViewPolicy::Conservative` is opt-in; `History` remains the default. The derived suffix
index keeps original argument locators, exact raw tool artifacts, distinct errors/effect outcomes,
and an incremental source-root identity. Only proven identical successful inspections coalesce.
The compiler prioritizes the active working dependency frontier, retains all distinct supplied
source evidence conservatively, uses the negotiated envelope, and preserves the complete current
native exchange. Opaque/native dependencies explicitly retain the current history selector.
Adapter-envelope failure also falls back when current restrictions allow it; restricted views
instead page readable sources and withhold unscoped derived/native text.

Model scope is not an independence certificate. Derived projections state whole-view/candidate
dependencies, every supplied file read is bound even if undeclared by a model, and excluded source
bytes cannot return through unknown derived/native dependencies. C6 canonical entries are not
rewritten. Full managed candidate observations, current admission and qualification remain.
Private versioned reconstruction identities include current input/control, authority, candidate,
all supplied file versions, pending state, profile/capabilities, policy and ordered tools. The
complete final ModelRequest fingerprint additionally includes options, reasoning and tool choice.
These identities do not authorize reuse of a request or a verdict. Diagnostic-off skips key work.

`context_sources` adds byte-bounded, scope-bound, fixed-snapshot descriptor paging; each page
rechecks current authority. Exact originals remain available via the named retrieval tool.
Plan-step descriptors identify `inspection_result_read` rather than pretending that their raw
child JSON is the parent host-record byte stream. Existing v3/page/event encodings remain intact;
plan execution still independently requires its existing v4 reader guard.

- `candidate/i4-decision-resource-replay/result.json` passes seven actual public-API groups:
  exact facts/current host state/native closure/broad dependencies; two same-size restored-mtime
  changes and a mode-only change in an undeclared dependency; complete bounded descriptor
  paging and distinct failed attempts; live opaque/read restrictions with no derived quotation
  of excluded bytes; independent reviewer namespace and no writer scratch; current-state
  history rollback; and exact original-byte continuation for a large partial raw JSON card.
  Independent source/artifact hashes, UTF-8 ranges and current native facts are checked. No
  inference was used. Consumer SHA-256:
  `8be4cf45965e818338d72bc390f022f3d8b94ba3516cb48fd79fa2647952bdee`.
- The first real-provider chain pair (`candidate/i4-decision-live-chain/results.json`) reads
  eight actual linked JSON files in chain order, returns all independently generated exact
  values and preserves every file byte. Both use nine requests/eight host calls. History used
  149,770 input tokens in 67.262 seconds; conservative used 149,038 in 70.494 seconds. Both
  reported zero cached input and 886 output tokens. The 0.49% input reduction does not establish
  a speed benefit: elapsed time regressed in this sample. All nine final-request bindings
  matched their committed views. Binary SHA-256:
  `b785ab6366ee0f1564c7cf80080f2d11fab7c023d835126397587b993af32f76`.
- The implementation cohort builds daemon/CLI/TUI with two workers (`i4-decision-build-r8.log`),
  passes affected all-target/all-feature strict Clippy (`i4-decision-clippy-r3.log`) and full
  workspace formatting (`i4-decision-fmt.log`). New compiler navigation, source-tool/configuration
  documentation and retained real-interface reproducers describe their bounded coverage.

Comparable live repeats and one actual review conversation now pass, as recorded below. These
results do not qualify all I4 acceptance, candidate review/finding conservation, all providers
or platforms, or the still-open all-stage goal.


### I4 live repeats and actual review

Three paired actual-provider chain runs (one pair in reversed order) returned every independent
value with unchanged bytes. History and conservative each used 27 requests. Total input was
449,136 versus 446,914 tokens (0.495% lower); reported cached input was 15,872 versus 47,616.
Output was 2,591 versus 2,623. Median elapsed time was 63.928 versus 68.614 seconds; the compiler
was slower in all three pairs. Vendor caching was observed rather than assumed absent.
`candidate/i4-decision-live-summary.json` retains every case/identity. This workload does not
justify enabling the compiler or claiming an end-to-end speedup.

`candidate/i4-decision-actual-review/result.json` uses actual Codex review inference after
source-backed writer-only scratch was committed through C6. The reviewer independently reads
the real source and returns its exact independent value. Read-only inspection of the published
review view contains the fact and no writer scratch; both namespaces remain on disk. Two
requests report 31,253 input, zero cached input and 190 output tokens. Both final request
bindings match committed views. This covers a read-only review conversation, not candidate
review/finding conservation. Current consumer SHA-256:
`77edfdae84a4921f71117eae1bb7c01536c3e2fa57ffacfacc0d2023fff3b852`.

## I6 shared full-capture experiment

The managed recorder formerly captures repository/source/repository independently.
`candidate_observation="shared"` derives both unchanged digest encodings from one complete
captured preimage, then requires a second fresh full capture to match both. Every material
boundary retains current full scans and the same retry limit. No prior-boundary candidate,
notifications or metadata-only tokens are reused. In-place scopes retain their existing path.
Default `"separate"` retains the original implementation and independent rollback without
migration. Candidate gates, independent review and settlement remain required.

`candidate/i6-shared-observation-final/result.json` passes 15 paired static runtime groups:
baseline, same-size/restored-mtime bytes, non-executable modes, root permissions, untracked
additions, deletion, regular-to-symlink and target changes, real staged index preservation,
same-source root HEAD changes, nested repositories, nested byte/mode/HEAD changes, and unborn
nested repositories. Both identities match the existing path exactly. Source bytes/kinds/modes
and real indexes are independently unchanged by observation. A tracked FIFO produces the same
explicit rejection. The initial fixture wrongly assumed Git enumerates an untracked FIFO;
that failure remains in `i6-shared-observation-replay`. The corrected fixture enrolls the exact
unsupported path in the real Git index before demanding rejection.

Twenty-four observations under a real concurrent atomic publisher all return one of the two
independently preobserved exact identity pairs. No mixed/invented identity appears, no temporary
file enters the workspace, and the real index remains unchanged. This is bounded race coverage.

Twelve warm paired samples on 256 independent 64-KiB files, alternating configuration order
without an overlapping build/campaign, measure median full-fence time 0.125701 seconds for the
original path and 0.087130 for shared (30.7% lower). Native Git traces independently count three
complete root source enumerations versus two per accepted observation. Tracing is symmetric;
Git objects and OS caches are warm. This is not a cold-disk or whole-product speed claim.
`i6-shared-observation-correctness` overlapped a build and is functional evidence only. Final
runtime consumer SHA-256:
`cdd035fe7283ecd7d11cded560818625ff3571a6d4a7ff1ec91d3ab83a7805bd`.

The cohort builds daemon/CLI/TUI with two workers (`i6-shared-build-r2.log`) and passes affected
all-target/all-feature strict Clippy (`i6-shared-clippy-r2.log`). Full managed product operation,
final regression/formatting, other platforms and the remaining I6 experiments remain open.


### Final I6 cohort and remaining R10 defect

Both full managed artifact journeys in `candidate/i6-shared-managed-artifact-live/results.json`
reach `Accepted` settlement through actual design/write, the existing CSV gate and typed
independent review. Exact CSV bytes, unchanged project manifest, original Git HEAD/index and
final request/checkpoint bindings are independently checked. Separate uses seven requests,
147,702 input, zero cached input and 1,269 output tokens; shared uses six requests, 125,110
input, 9,984 cached input and 888 output tokens. Observed elapsed times are 77.202 and 57.518
seconds. Different model call counts and one sample do not attribute a product speedup to local
capture sharing. The initial managed fixture had no artifact project anchor and correctly
returned `CandidateAvailable`; its evidence is retained in `i6-shared-managed-live`. The
corrected fixture supplies the existing `peritus-workspace.toml` artifact contract before the
run, without changing production gate requirements or bypassing acceptance.

The current cohort also passes all seven compiler groups (`i6-compiler-final-regression`) and
nine plan groups with the conservative compiler (`i6-compiler-plan-integration-corrected`),
including exact child bytes, restrictions, v4 older-reader refusal and execution-disabled
retrieval. The first integration invocation accidentally supplied the line-oriented legacy
consumer to the ordinary consumer interface; its ENOENT failure is retained separately.
`i6-shared-source-identity.json` records 4,836 source/resource/reproducer files, and
`i6-shared-binaries.json` records exact consumer identities. Full workspace formatting passes
(`i6-shared-fmt.log`). At that earlier checkpoint no stage was qualified complete; I0 is now complete below.

`candidate/i4-dependency-invalidation-before/dependency-invalidation.json` demonstrates the
remaining R10 defect against this same actual binary. A model-authored plan quotes the supplied
Beta fact but declares only Alpha validity. After a same-size/restored-mtime Beta change, the
reconstruction key changes and the exact source card is marked historical, but the derived
entry still reports `status=open`. Its broad host-dependency annotation alone is insufficient.
`decision-replay.py --require-host-invalidation` preserves this failing real-interface oracle.
This requires a durable host-enforced dependency binding and conservative treatment of legacy
or unknown conclusions, not another label or a narrower model declaration. Existing C6
`WorkingValidity` supports combined constraints/uncertainty, but its model-delta reducer refuses
newly stale entries; a proper host operation and compatibility plan are needed rather than
misusing a model delta or synthesizing an environment change. The compiler remains opt-in;
R10 and I4 remain explicitly incomplete. The goal remains active.


Final current-cohort default-path replays pass: nine varied public-context case groups (264
interface actions) in `i6-shared-varied`, and eight artifact integrity/restart/rollback/older-reader
case groups in `i6-shared-page-integrity`. Rejection return codes in the integrity campaign are
expected negative outcomes; its complete result passes. These are correctness replays; their
execution overlapped and does not supply timing evidence. The source/identity coverage record
is a hash inventory, not a claim that all 4,836 files were read or audited. This increment traced
the context compiler/update and C6 validity/delta boundaries, provider request binding, managed
capture/runtime and candidate recorder callers, plus the documented recovery/rollback paths.


The design now records a concrete I4 follow-up: an explicit host C6 dependency operation,
actual request-view binding, conservative legacy/unknown staleness, atomic retention of new
unknown conclusions, and a v5 reader guard before new host events. This is a planned amendment;
current production code still supports checkpoint versions 1–4 and still reproduces the R10
failure. State/page codecs can remain unchanged. The pipeline design hash is synchronized;
no host event, schema-5 implementation, current-state migration, commit or push is claimed.


## I0 completion audit

I0 is complete. Its stage-specific contract and evidence are recorded in
[the completion report](../docs/testing/execution-efficiency-i0-cost-baseline.md). This does not
mark the full R01–R16 / A01–A15 objective complete. The ledger now distinguishes completion
of one increment from qualification of all later optimizations.

The final current-build campaign passes seven real cases: diagnostics-on/off chat with actual
working-state creation, reversed-order diagnostics-on/off independent review, genuine unsupported
model failure, in-flight native cancellation, and a managed CSV edit through `Accepted`
settlement. Nineteen canonical responses are independently decoded; 17 successful responses
have known usage and two negative responses remain explicitly unknown. Fourteen diagnosed
admitted attempts match cumulative accounting; five other responses intentionally have no
diagnostic files. The paired initial canonical views are byte-identical and answers/files are
independently checked. All seven rendered component classes occur and their byte/character
partitions reconcile exactly; telemetry contains no fixture nonce/body or workspace path.

The managed case independently verifies first actual change (22.530 seconds), acceptance
(68.745 seconds), current gates/review, exact output and unchanged original HEAD/index/manifest.
Build, affected strict Clippy, workspace formatting and diff checks pass. No speedup is claimed
for instrumentation. Native OpenCode live correction/layout, other platforms and the broader
later-stage campaigns remain unqualified; unavailable counters are not zero or passing samples.

Two I0 gaps found during completion were repaired: diagnostics-off still wrote the diagnostic
state sidecar, and native correction usage could disappear from diagnostics when a later
inference failed. Sidecars now stay absent when disabled; each observed native projection
retains its own usage/outcome before logical combination. Renderer measurements now include
exact character counts and component spans without an additional full serialization. Actual
role, turn, retry attempt, reason, top-level tool duration and checked-source lookup counts are
recorded. These additions do not select or enable an optimization.

The I4 host-dependency prototype was set aside when the user limited work to I0. Its code builds
and its first local counterexample replay passes, but publication/recovery/privacy and sustained
qualification remain incomplete; the planned v5 amendment is not signed off. Other stage
statuses remain unchanged. No stage beyond I0 and no whole-codebase review is claimed complete.


## I1 completion audit

I1 (phase two) is complete; two of the seven increments are now qualified. The implementation,
measurement boundaries, regressions/variance and exact evidence paths are recorded in
[the I1 completion report](../docs/testing/execution-efficiency-i1-indexed-projections.md).
I2–I6 and the full R01–R16/A01–A15 objective remain incomplete. No later stage is resumed.

The final same-build reference preserves the already constant-time direct lookup and policy
prefix. It restores the actual former linear ranking search, repeated sort-key evaluation,
full fact/inspection filtering, tool identity/sequence scans and byte summation. Earlier global
linear-lookup stress samples are retained but excluded from the performance baseline.

Two 320-read writer pairs alternate selection order and reach 671 retained observations; a
third pair covers the separate reviewer lineage. All 318 answers/actions correspond exactly,
including full tool arguments, rejection reasons, pending/settled state and recovered views.
Independent fixture bytes and hashes verify 3,096 source deliveries. Named archive/ranking
visits fall from 46,736,629 to 155,324 in both large pairs, with 151,876 versus 9,276 rank-key
calculations. Warm assemble-plus-checkpoint medians improve about 33% and 9%, with substantial
absolute timing variance; no universal or provider-latency improvement is claimed.

Two actual Codex chat runs return exact independent answers with identical initial canonical
requests and reconciled usage. Six actual-native-trace cases prove identity deduplication,
conflicting-outcome/sequence-gap rejection and unchanged committed roots. The retained varied
campaign passes nine groups/264 actions; checkpoint integrity passes eight cases, including
an older reader and corrupt-page rejection. Build, strict affected Clippy, workspace formatting,
telemetry privacy and diff checks pass. The reference setting reopens current history without
a format migration. No commit, push or deployment was performed.


## I2 completion audit

I2 (phase three) is complete, with page reuse enabled by default. Three of seven increments are
now qualified. [The I2 completion report](../docs/testing/execution-efficiency-i2-checkpoint-pages.md)
records the complete contract, evidence paths, boundaries and explicit rollback setting.
I3–I6 and the whole-design objective remain incomplete and are not resumed by this instruction.

The earlier timing regression was reproduced in forward order and reversed in reverse order;
whole-cohort ordering dominated that comparison. A final same-build, alternating comparison
checks exact views, full arguments/outcomes and canonical component bytes at 1,139 retained
observations. Every one of 190 published checkpoint root sets is independently complete.
Measured writes fall from 30,877,714 to 2,222,028 bytes and finalizations from 960 to 320;
observation/index encodings fall from 192 each to 32 each, with 160 verified page-pair reuses.
All 32 matched local operation pairs favor reuse (617.206 vs 424.570 ms medians). Separate
syscall traces prove each selected sealed artifact is physically written once at formation.
Full verification/root work remains; no whole-application or provider-speed claim is made.

A real warm-cache replay exposed matching-byte symlink acceptance in the original verifier.
The shared file-hashing boundary now checks regular object type, matching the existing reader.
The original defect is retained with the prior binary and a permanent replay. Fourteen warm
corruption cases and eight cold/older-reader cases pass. Fourteen debugger-kill/publication-failure
cases preserve the appropriate committed root and recover exact completed source observations
without redispatching tools or inventing provider submissions. Both checkpoint boundaries,
fsync, CAS, lineage/version checks and all current dependency roots remain intact.

Real Codex chat/context updates pass in both selections, and both managed CSV edits reach
Accepted with exact outputs and unchanged original Git HEAD/index/manifest. Usage reconciles
against canonical C5 independently. The extra native request in the managed reuse sample and
its higher elapsed/token cost are retained, not hidden. Varied APIs, build, strict Clippy,
workspace formatting and diff checks pass. No schema migration, commit/push or deployment is claimed.


## I3 completion audit

I3 (phase four) is complete, with once-only OpenCode schemas enabled by default. Four of seven
increments are now qualified. [The I3 completion report](../docs/testing/execution-efficiency-i3-provider-layout.md)
records the actual native comparison, exact coverage, source/artifact identities and rollback.
I4–I6 and the whole-design objective remain incomplete and are not resumed by this instruction.

The earlier credential gap is resolved using the existing dedicated private test credential and
official OpenCode 1.19.0 runtime. No saved user route, key or service is modified. Twenty-five
actual C5/ProductRunner cases and nine public route cases pass: automatic/required/specific/none/
unavailable selection, one/two-call limits, terminal turns, default and model reselection, changed
developer metadata, actual malformed correction and bounded persistent failure. Thirty-one
native inferences retain their real results and usage. Rejected malformed turns dispatch no
host tool. Full argument schemas/descriptions match the host definitions exactly, with unchanged
system/role origins and host validation. Explicit legacy remains available against current history.

Two defects found during qualification are fixed: model selection dropped the explicit layout,
and native fresh input was incorrectly reported as total input when cached partitions existed.
Actual cached native JSONL now reconciles to full normalized input, individual projections,
canonical C5 and cumulative accounting. Unknown logical/tool/provider-cost counters remain unknown.
The original layout failure and an unsuccessful fault induction are retained separately.

Two alternating-order ProductRunner chat pairs preserve exact independent answers, original
file bytes and initial logical views, with equal request counts and no natural correction.
Combined input falls 41,780 to 28,529 tokens (31.7%); complete input/output falls 30.8% and adapter
bytes 32.1%. Median latency is 3.1% higher with mixed paired results; no general speedup is claimed.
The direct all-tool control removes 41.6% of rendered bytes. The volatile developer marker still
changes system byte 1,210 before the schema, so there is no longer-prefix or inferred cache-hit
claim. Reported vendor caching is separate. Native Go/Windows/other models and managed edits are
outside this increment's live coverage. Build, strict affected Clippy, workspace formatting and
diff checks pass. No commit, push, deployment or later-stage completion is claimed.

## I4 completion audit

I4 (phase five) is complete for its increment contract. I0–I4 are now qualified; I5–I6 remain
incomplete and the broader goal remains paused. The latest instruction authorized this increment.
[The I4 completion report](../docs/testing/execution-efficiency-i4-decision-views.md) records the
implementation, exact formats, saved failures, replay commands, source/artifact identities and
rollback. The compiler is a qualified opt-in (`decision_view = "conservative"`); History remains
the default because the observed benefit varies by workload.

Host-bound working entries retain their actual originating request and conservative dependency
closure. A new public assembly cannot inherit a prior request binding. Unknown, direct, external,
ignored, directory/search and pending inputs require recheck. The compiler preserves the last
complete native exchange plus following host messages. Declared completed progress survives
stale freshness instead of becoming unfinished work. Four concrete pre-fix regressions are saved.

Checkpoint v6 is committed before new host semantics; v5 remains readable and migrates
conservatively. Host events use C6 v3 and bound entries use v4, preserving declared status.
Old readers and old entry cursors reject explicitly. Full validation, fsync, CAS, both checkpoint
boundaries, current authority, source paging and independent qualification remain enforced.
History rollback uses current committed state and retains the reader floor and sticky staleness.

The final-build public-interface campaigns cover exact facts/paging, unknown and altered bindings,
completed plans, ignored reads, graph/model-boundary rejection, publication failure, real native
process kills, older-format migration, sequential-plan interoperability, 264 varied API actions,
and current input invalidation. Actual providers exercise changed-source continuation, isolated
review, paired token observations and an Accepted managed CSV edit. Full details and bounded
provider/platform coverage are in the report; a live nonempty finding/fixer cycle is not claimed.
Final-build paired inventory input falls 6.5% with equal requests and host operations; the
simpler chain saves 2.5% input but is slightly slower. No universal speedup is claimed. Build,
strict affected Clippy, workspace all-target/all-feature checks, formatting and diff checks pass.
No commit, push, service change, deployment or later-stage completion is claimed.

## I5 completion audit

I5 (phase six) is complete for its sequential plan/wait contract. **I0–I5 are qualified; I6 remains
incomplete.** The broader goal remains paused beyond this authorized increment.
[The I5 report](../docs/testing/execution-efficiency-i5-plans-waits.md) records the architecture
verdict, saved failures, exact cohorts, replay commands, artifact identities and bounded coverage.

Actual native-provider daemon runs prove steering, read revocation, pause and cancellation stop
later plan children. The original command survives wait steering, permission change, pause and
cancellation. Passive checks add no tool reservations. A full daemon crash retains the first
child and accounting, reports RecoveryRequired and submits no new provider request or child.
Reviewer startup/parent/child operations are charged to Reviewer, with zero Writer tool calls.

Qualification found and fixed an ignored process-permission change during a live wait. Current
effective bits and the existing durable policy revision now fence the observation; revoke then
restore still yields even when the original bits return. Static ConversationView implementations
remain compatible. A real DeepSeek ambiguous literal/reference proposal also exposed missing
tool guidance. Referenced fields must now be visibly omitted, including placeholders; the whole
invalid graph still rejects before dispatch. A final live regression also caught before-edit
being projected as a full pause. Passive control now preserves read admission under BeforeEdit while the unchanged mutation reservation still
blocks edits. Neither admission nor result values are weakened.

Interactive output waiting, exact Unicode stdin and an independently released active terminal
wait use one native process with no model polling. Actual kills before/after plan child commits
and terminal projections, SQLite refusal, projection-write failure, native death and foreign
owners preserve exact committed evidence or explicit uncertainty without redispatch. The older
reader refuses v4; disabling plans retains current data and exact result retrieval. All existing
validation, fsync, CAS, batch/request checkpoints and acceptance boundaries remain.

Final real-provider comparisons preserve both actual inspections and every source byte. Each
task uses two requests with a plan instead of three with direct tools. Two alternating Codex
pairs use 29.6% less full input; one independent Zen/DeepSeek pair uses 24.5% less. Output is
higher and small-sample observed latency is lower; there is no universal speedup claim. Plans
stay disabled by default. Build, affected strict Clippy, workspace all-target/all-feature checks,
formatting and replay checks pass. No commit, push, live service change or I6 completion is claimed.


## I6 completion audit and final traceability

I6 (phase seven) is complete. **All seven increment contracts I0–I6 are complete.**
[The I6 report](../docs/testing/execution-efficiency-i6-freshness.md) records the final cohort,
platform fallback, exact capture/restart behavior, native outcomes, costs and reproducible scope.

The managed recorder derives both existing identities from one full capture and requires a
second fresh pair to match. A changed pair retries; errors and uncertainty remain explicit.
Sharing is opt-in on Linux; Separate remains the default/rollback everywhere, and other
platforms use the original fence even if Shared is requested. In-place ownership and fresh
candidate gates, review and settlement remain unchanged. No identity or continuation format
changes. Serial reads and complete catalogs remain the selected implementation; optional
parallelism/schema-pack experiments are not introduced without additional benefit evidence.

The final cohort passes eighteen static correspondence groups, 24 actual concurrent-writer
observations, a real linked worktree, unsupported-node rejection and actual Git object-write
refusal/recovery. Native kills between captures and after the second capture recover through
fresh exact observations; an inter-pass same-size/restored-mtime amendment forces attempt two.
Six final native journeys reach Accepted: both managed policies, in-place scope, fresh-process
rollback, source amendment and HEAD amendment. All 46 actual provider responses reconcile to
canonical C5 and current request/view bindings. The source amendment re-enters writer work and
reads the amended bytes; unchanged/HEAD-only continuations recheck without another writer turn.

Twelve alternating warm pairs on 16 MiB measure three full enumerations versus two, and median
local observation time 0.127375 versus 0.087773 seconds (31.1% lower). Native task timing is mixed;
there is no general product speed or token-saving claim for this increment. The application
build, affected strict Clippy, all-target/all-feature workspace check, Windows cross-check,
format and replay checks pass. Apple SDK availability blocks macOS compilation; macOS retains
Separate and has no native pass claimed. The report records corrected fixture/oracle failures.

The following is requirement/journey traceability, not a claim of exhaustive provider, platform,
mode or whole-codebase coverage. Each linked report states its exact qualified boundaries.

| Requirements / acceptance journeys | Implementation and retained evidence |
| --- | --- |
| R01–R02; A01 | Current authority, complete tools and independent acceptance remain. Actual chat/review/design/write and managed/direct interfaces are covered by [I0](../docs/testing/execution-efficiency-i0-cost-baseline.md), [I3](../docs/testing/execution-efficiency-i3-provider-layout.md), [I5](../docs/testing/execution-efficiency-i5-plans-waits.md) and [I6](../docs/testing/execution-efficiency-i6-freshness.md). |
| R03–R05; A02–A04 | Exact retained pages/roots, durable boundaries, real publication refusals, corruption and native kill/recovery cases in [I2](../docs/testing/execution-efficiency-i2-checkpoint-pages.md), plus host-delta and plan/command interruption in I4–I5. No lifetime quota or weaker fsync/CAS is introduced. |
| R06; A05 | Exact indexed/reference selections, suffix work, rejections, 318 actions and 3,096 source deliveries in [I1](../docs/testing/execution-efficiency-i1-indexed-projections.md). |
| R07–R08; A06–A07 | Actual OpenCode choices/layout/correction and final provider framing/cache accounting in [I3](../docs/testing/execution-efficiency-i3-provider-layout.md); current volatile input and request binding remain in I4. |
| R09–R10; A08–A09 | Exact decision views, durable host dependency widening, undeclared/ignored/unknown changes and real same-lineage continuation in [I4](../docs/testing/execution-efficiency-i4-decision-views.md); full fresh candidate and source/HEAD restart checks in I6. |
| R11; A09 | Same-boundary full capture sharing, unchanged material gates, inter-pass retry, kinds/modes/nested Git changes and conservative platform fallback in [I6](../docs/testing/execution-efficiency-i6-freshness.md). |
| R12–R13; A10–A12 | Sequential exact plans, result paging, current child admission, real steering/pause/revocation, original-handle waits and native owner/projection recovery in [I5](../docs/testing/execution-efficiency-i5-plans-waits.md). |
| R14; A13 | Actual reviewer scratch exclusion and source reads in [I4](../docs/testing/execution-efficiency-i4-decision-views.md), real reviewer role/counter binding in I5 and current candidate acceptance in I6. |
| R15; A14 | Actual older-reader refusal and supported-format migration, independent current-state rollback and retained original command ownership across I2–I6. |
| R16; A15 | Canonical cost baseline in I0; matched observations, exact outcomes, per-provider usage, rejected/retried work and explicitly bounded local/native timing in every increment report. Savings are not pooled across providers or multiplied across increments. |

No required increment work remains. All changes remain local and uncommitted; unrelated work,
Git HEAD, live service configuration and credentials are preserved. No release or deployment
is part of this completion.
