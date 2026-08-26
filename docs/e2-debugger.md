# E2 debugger

`peritus-debugger` is the durable diagnostic boundary for Project Peritus. It reads immutable C7
trace projections and checked C0 evidence, binds them to the exact E0 run, D0 attempt, C6 context,
provider profile, environment, workspace, and E1 harness revision, then produces bounded causal
timelines, failure hypotheses, cross-run patterns, component correlations, harness-health
summaries, and citation-complete reports.

E2 is deliberately non-authoritative. A report may describe observations, interpretations, and
recommendations. It cannot mutate an E1 harness, write a workspace, run an evaluation, waive a
finding, accept a run, promote a revision, or move a production pointer.

## Job input and binding

A debugger job freezes these inputs before selecting evidence:

- a stable job and command identity;
- one or more exact diagnostic subjects;
- E0 run, D0 attempt/session, workspace and environment identities;
- the shared revision tuple plus the full branch-distinguishing E1 harness revision identity;
- the C6 context/render-plan and provider-profile identities when model analysis is enabled;
- a checked query, deterministic or model-assisted mode, and retry policy; and
- independently bounded selection, analysis, model, report, event, and state limits.

Bindings are structural, not advisory. Drift in any named run, attempt, workspace, environment,
provider, harness lineage, logical revision, full revision digest, or component digest rejects the
job before evidence selection. A job never silently rebinds to a newer revision.

## Evidence selection

Selection consumes a checked `TraceProjectionState` and C0 integrity export. Queries can constrain
subjects, attempts, observation kinds, time intervals, trace/span identities, and causal closure.
The selector orders matches canonically and emits one immutable `TraceSelectionManifest` that
records the exact journal position, event and trace identity, causal predecessors, frame digest,
frame length, and subject binding of every selected item.

Causal closure can add only transitive same-subject ancestors already backed by the checked C0
export. Missing rows, corrupt frames, cross-subject parents, invalid bindings, and bound exhaustion
reject the complete selection. There is no partial-success manifest. Default selection uses C7's
redacted observations and never dereferences raw-vault references.

## Deterministic analysis

The deterministic pipeline performs four distinct operations:

1. Normalize task outcomes separately from infrastructure outcomes.
2. Build canonical per-attempt causal timelines from selected events.
3. Produce ranked root-cause candidates while preserving alternatives, contrary evidence,
   ambiguity, derivation, and bounded confidence.
4. Cluster repeatable success/failure patterns and map them to exact E1 components or explicitly
   class-only component kinds.

The closed taxonomy covers specification, context/provenance, model behavior, provider transport
and accounting, tool routing and execution, workspace/Git state, process/sandbox/platform state,
durability/replay, scheduling/collaboration/orchestration, gates/review/acceptance, harness
composition, telemetry/evidence, resource exhaustion, cancellation, and unknown-but-observed
failure. Unknown does not mean unrecorded: it retains the evidence and ambiguity without inventing
a category-specific claim.

Pattern fingerprints derive from normalized typed facts rather than prose. Exact canonical inputs
and limits therefore yield identical manifests, timelines, cause ordering, cluster membership,
component correlations, health summaries, report bytes, and report digest regardless of map or
input iteration order.

## Citations and reports

Every observation claim cites selected evidence. Every inferred cause cites supporting evidence,
retains contrary citations where present, and identifies its derivation. Recommendations remain
separate from observations and inferences and cannot carry executable operations or authority.

A citation can name only a selected trace event or a nonempty in-range span of a selected finalized
C0 artifact. Validation reruns subject containment, canonical ordering, taxonomy, bounds,
timeline, causes, clusters, component links, health summaries, and citation checks. Only a
validated report can be canonically encoded, finalized in the artifact store, and admitted to the
evidence catalog. Original trace/evidence records are never overwritten or annotated in place.

## Optional model analysis

Model assistance is provider-neutral and optional. E2 consumes a frozen C6 context/render plan and
an already configured C5 `ModelProvider`; it does not load credentials or choose provider-specific
transports. Render segments remain separate messages with their trust and provenance boundaries
intact.

The response must contain exactly one strict structured result. E2 reduces the C5 stream, parses
the result into closed debugger types, and reruns all selection, taxonomy, citation, subject,
authority, and report checks. Text-only output, tool calls, refusal, malformed streams, unsupported
fields, invalid citations, over-limit content, or attempted authority are rejected as a whole.
Model proposals may add competing hypotheses, explanation, and recommendations; they cannot erase
deterministic findings, hide contrary evidence, change a binding, or manufacture acceptance.

## Durable protocol

The E2 aggregate uses:

| Purpose | Stable identity |
|---|---|
| debugger command frame | B3 family 82, schema 1 |
| debugger event frame | B3 family 83, schema 1 |
| complete debugger checkpoint | B3 family 84, schema 1 |
| C0 aggregate kind | `Debugger`, tag 14 |
| C0 checkpoint namespace | `0xE201` |

Decoded frames are inert. Checked constructors and the reducer revalidate semantic state before a
transition is admitted. Unknown closed tags, unsupported versions, invalid lengths, noncanonical
ordering, truncation, and trailing bytes reject.

The reducer records selection, deterministic analysis, model request/result settlement,
cancellation, report completion, artifact publication, and evidence admission as explicit phases.
Each transition binds its sequence, predecessor, prior-state digest, command identity, and complete
checkpoint. Exact retries resolve through command idempotency; conflicting retries quarantine or
require replay.

## Restart and publication

Recovery replays the contiguous family-83 event chain through the same reducer and compares the
result with the complete family-84 checkpoint. Pending outbox work is classified from durable C0
state instead of guessed from process memory.

Publication follows C0's artifact-dependency ordering while keeping logical publication
commit-before-effect:

1. Validate and hash the complete canonical report.
2. Idempotently finalize the content-addressed bytes as prepared, still-unreferenced content.
3. Commit `CompleteReport` with that exact finalized artifact dependency and the stable publication
   directive; this is the authoritative report-commit position.
4. Claim the directive and admit an evidence record citing that position and artifact root.
5. Commit `RecordPublication` with the evidence identity and acknowledge the exact claimed fence.

A crash after artifact preparation reuses the same digest or allows ordinary unreferenced-artifact
collection; it cannot manufacture a report. An ambiguous artifact/evidence boundary remains
reconcilable and does not create a second logical report. Cancellation is also durable and
terminal; a late model result or publication response cannot turn a cancelled job into success.

The rebuildable projection exposes bounded status, immutable query/selection/report digests,
progress, budget use, retry state, typed safe failures, and artifact/evidence identities. It never
exposes credentials, raw-vault bytes, capabilities, evaluation results, or production pointers.

## Schema migration

C0 schema version 7 widens only the constrained journal aggregate-kind columns from tags 1–13 to
1–14. It copies heads and events in canonical order, verifies row counts, rebuilds the command
index, and publishes schema/user version 7 in one transaction after a completed whole-file backup.

The frozen v6 fixture proves that every historical tag and event frame remains byte-exact. The
upgrade test then appends tag-14 family-83 data, runs the journal integrity scanner, restores the
backup, and verifies the original v6 rows and `user_version`. Once real tag-14 data exists, an old
v6 binary must not open the store; use the verified backup or a future forward repair.

## Verification and operation

A2's nonempty debugger suite independently exercises evidence selection, timelines, taxonomy,
citations, invalid model output, clustering, replay, cancellation, malformed frames, redaction,
resource bounds, panic containment, and teardown isolation. Formal obligations cover selection
and citation containment, report validity, replay equivalence, bounded analysis, and absence of
mutation or authority.

Run Cargo, Verus, xtask, and `just` commands serially with `CARGO_BUILD_JOBS=1`. Focused E2
verification covers the debugger crate, B3 generated artifacts, C0 migration/journal/projection/
artifact/evidence boundaries, A2 conformance, architecture and source policy, ordinary API audit,
trust accounting, proof impact, and reproducibility. The merge authority remains one complete
local Gate A followed by hosted Gate A and Foundation matrices on Linux, macOS, and Windows.

Operationally:

- preserve exact job and command identities during retry or reconciliation;
- repair missing/corrupt evidence instead of producing a partial report;
- treat diagnostic confidence as bounded evidence strength, not acceptance probability;
- disable optional model analysis without disabling deterministic analysis or replay;
- retain prior reports and evidence rather than rewriting historical diagnosis; and
- route proposed harness changes through later E3/F0 authority, never through E2.
