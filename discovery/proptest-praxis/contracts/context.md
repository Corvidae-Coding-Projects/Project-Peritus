# Stage 1 working-context closure pilot

Baseline: `d106c9b13318fbcdd3d816ed63ae48532806fa7e`. Standalone discovery consumer;
no retired test crates, production hooks, test-only authorization bypasses, or production edits.

## Public boundary and source grounding

The target is `peritus_context::working`: `WorkingState::new`, observation ingestion,
`WorkingDelta::new`, `apply_working_delta`, `render_working_state_complete`,
`render_working_state`, and `render_working_state_with_headroom`.

Read these current implementations:

- `crates/orchestration/peritus-context/src/working/{binding,limits,entry,evidence,validity,state,state_access,delta,validation,selection,error}.rs`.
- `crates/orchestration/peritus-context/src/selection/{plan,closure,ordering}.rs` and
  `src/{precedence,render,budget,content,identity,lib}.rs`.
- `crates/orchestration/peritus-role/src/{harness_role,context_policy}.rs` and
  `context_policy/tables.rs` for the explicit reviewer exclusion contract.
- Caller/source construction: `crates/app/peritus-product-runner/src/local_context/tools/update.rs`
  constructs source-backed entries and applies host-checked validity; memory ingestion and
  `local_context/port.rs` construct the same scope/environment concepts.
- Current product presentation: `local_context/assembly/working.rs::append_with_budget`
  independently ranks and serializes a provider-sized JSON view. It does **not** call the
  working render functions exercised here. This pilot does not qualify product request assembly.

Errors exercised are wrong task binding, stale reducer revision, dependency cycles, zero
selected allocation, and required dependency closure exceeding the hard token ceiling.
Each rejected delta is followed by another real complete render of the unchanged state.

## Independent contract

The case records roles, initial logical DAG, explicit entry kinds/status categories, content,
construction history, and later operations. Open plans, unresolved failed approaches and
contradicted records are required roots. Resolved plans and open assertions are optional.
Required roots include all transitively reachable prerequisites exactly once. Reviewer memory
is wholly excluded. Other harness roles can render this non-authoritative memory.

The oracle runs breadth-first traversal over case-defined adjacency lists and `BTreeSet`
membership; it uses no production closure, planner, certificate, model, or `verified` helper.
For optional nodes all generated authority/priority/source-recency dimensions tie, so ascending
stable ID is the documented admission tiebreak. The oracle tries whole closures in that order
and admits them atomically, using set differences to avoid double-charging shared ancestors.

Serialization byte costs come from a separately checked complete rendering: every logical
entry must appear exactly once with unchanged text and non-authoritative evidence metadata.
The oracle derives costs as `ceil(rendered_bytes / 3)` and checks attached accounting. This
reuses rendered serialization length, **not** production selection or reachability decisions.
It therefore does not independently prove the entire textual working-entry formatter correct.
Direct and headroom allocations are exercised below, at, and above required/all-entry costs.

## Generator and behavioral metrics

Constructive generators produce edgeless graphs, chains, diamonds with shared ancestors, and
general bounded DAGs, five roles, batch/incremental construction, Unicode/escape text,
replacement after reads, bounded renders, and three distinct invalid update families.
Batch-generated chains and diamonds can reverse stable IDs, creating valid forward-ID
dependencies whose prerequisites occur later in the canonical delta. Incremental insertion
retains prior-ID prerequisites. A separate BFS cycle check validates replayed case inputs.
References regenerate from graph construction when the initial node vector shrinks. Replacement
indices resolve deterministically modulo nonempty retained length, so no generated operation is skipped.
Persisted cases retain all actual nodes and operation data independently of random seeds.

Fixed examples include both writer and reviewer empty states. Empty rendering has no plan or
omissions even at a zero allocation; this is checked before the nonempty allocation rules.
An explicit unequal `left` to `rght` replacement has the same UTF-8 byte length after initial
reads. `same_length_replacements` counts only unequal text with equal byte lengths, excluding
identical-content upserts.

Metrics distinguish actual complete/bounded/headroom renders, successful selection, zero and
required-capacity rejection, each stale/scope/cycle attempt, continued operation after rejection,
entry/edge/required-root/shared-prerequisite counts, omission, replacement, equal-byte replacement,
and batch versus incremental histories. Root orchestration must check these occur, not equate
an increased case count with broader interaction coverage.

## Calibration

`calibration/context.patch` deliberately changes working `Plan` importance classification to
`Observation` in an isolated calibration checkout. The production generic selector and its
certificate then both receive the wrong required/optional metadata; their agreement cannot
hide the error from this external case-derived oracle.

`calibration_case()` has an early optional distractor, an optional prerequisite, and a later
required plan depending on that prerequisite. Tight required-closure capacity must protect the
plan and its prerequisite. The mutation admits the distractor first and displaces the plan.
Root should show baseline pass, mutated failure, restored baseline pass, retaining exact JSON
case and failure. No deliberate production mutation has been applied by this worker.

## Gaps

No claim about host dependency capture, transitive workspace staleness, supersession, artifact
archive I/O, concurrent updates, durable wire round trips, live provider limits, or complete
ProductRunner request composition. Validity is explicitly task-local here. Content hashing
uses `peritus_codec::sha256` only to construct well-formed real inputs; hash correctness is not
the oracle. This is correctness discovery, not a context/token efficiency benchmark.

## Stage 1 execution

Fixed cases, smoke and two standard generated searches passed. The required-Plan classification mutation compiled, failed the explicit and generated cases, and shrank; both witnesses passed after restoration. Exact reports, compiler/source identities, timing and counters are in `../REPORT.md` and `../evidence/stage1.json`. These observations do not qualify the separate product request-assembly path.
