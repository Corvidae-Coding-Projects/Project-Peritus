# C6 context and memory

C6 is Peritus's pure, Verus-verified boundary for deciding what an agent may see, why it may see
it, how the material fits inside a model context window, and which derived memories are eligible
for retrieval. It is implemented in three orchestration-layer crates:

- `peritus-role` projects canonical B1 roles into context and presentation policy;
- `peritus-context` validates provenance graphs and produces bounded context/render plans; and
- `peritus-memory` validates derived-memory records and produces bounded retrieval/index plans.

The crates perform no I/O and hold no ambient authority. D0 will persist their inputs and outputs
through C0/B3, map render segments into C5 model requests, and mediate tools through C4.

## Authority boundary

`peritus-policy::ActorRole` remains the only security-role identity. C6 does not issue or consume
capabilities, change policy, accept work, waive findings, amend an acceptance specification, or
promote a harness.

`peritus-role` defines the writer, reviewer, fixer, evaluator, and evolution-agent context profiles
and restricted profiles for every other B1 role. Its capability view is only a read-only ordered
set of operation classes. Construction rejects any operation B1 does not permit, and the verified
subset predicate connects the executable result to B1's formal permission specification.

Reviewer context is always fresh and excludes producer ancestry, producer-hidden reasoning, and
memory-derived producer rationale. It contains the immutable specification, exact candidate,
relevant source, gate evidence, and prior finding/resolution evidence needed for an independent
read-only review. `ReviewIndependenceView` copies every B2 contract requirement and adds the C6
fresh-context requirement; it requests evidence rather than claiming that independence exists.

## Provenance-aware context

Context is a directed acyclic graph of bounded nodes, not a concatenated prompt. A node binds its
identity and content digest to:

- provenance such as system, application, user, repository, external, memory, tool, agent, review,
  or derived compaction;
- an independently checked authority and trust ceiling;
- a semantic context class used by role policy;
- required/optional selection mode, explicit priority, token estimate, and recency;
- exact role visibility; and
- a canonical dependency list.

Constructors reject inconsistent provenance/authority/trust combinations, empty or oversized
content, zero estimates, self-dependencies, duplicates, and noncanonical collections. Graph
construction rejects duplicate identities, missing dependencies, and cycles before selection.

Repository instructions, fetched text, tool results, model output, reviews, and memory can contain
instruction-like prose, but parsing prose never raises its authority. The metadata supplied by a
checked constructor determines its ceiling, and rendering keeps every source boundary explicit.

## Selection and token planning

The context selector operates deterministically:

1. apply the selected role's visibility policy;
2. calculate the complete dependency closure for required nodes;
3. fail with a typed error if any required node is hidden, incomplete, or over budget;
4. rank optional roots by the documented integer precedence tuple;
5. admit an optional root only when its entire not-yet-selected dependency closure fits; and
6. retain an explanation for every selected or omitted node.

The token budget records context-window capacity, reserved model output, reserved protocol
overhead, usable input, selected input, and remaining input. All arithmetic is checked. A plan is
returned only when required closure and accounting are complete; there is no partial-success path
that silently drops policy or specification material.

The planner accepts caller-supplied token estimates. D0 will obtain estimates from the selected C5
provider/tokenizer profile, but C6 remains deterministic and provider-neutral.

## Compaction and rendering

Compaction is an explicit derivation. A proposal identifies a new node, a compaction-policy digest,
the proposed bounded content, and canonical nonempty ranges from selected source nodes. Validation
rejects missing or hidden sources, invalid or overlapping ranges, digest/lineage errors, cycles,
and proposals that do not reduce the replaced token estimate.

Immutable policy, acceptance specifications, active user instructions, capability facts, and
unresolved blocking findings are protected from summarization. A successful derivation retains
links to every source and never raises the maximum authority or trust of its inputs. Validation
proves admissible lineage and bounds; it does not pretend that a prose summary is automatically
true.

A render plan is an ordered list of typed segments carrying source identity, message role,
provenance, authority, trust, context class, digest, and bounded content. Provider-specific message
encoding is deliberately deferred to D0's adapter into `peritus-model-protocol`.

## Derived memory

Memory records are immutable derived claims. Each record binds:

- a stable caller-supplied identifier and revision;
- exact project/workspace/repository/actor/role scope;
- canonical source events and supporting/contradicting evidence;
- claim type and original source provenance;
- bounded confidence, relevance features, token estimate, and feedback;
- explicit logical creation/review/expiry observations; and
- active, quarantined, expired, or superseded lifecycle state.

Logical observations are supplied by the durable caller; the crate never reads the wall clock.
Confidence and ranking use bounded integer scores rather than platform-sensitive floating point.
Constructors reject empty scope/source data, evidence conflicts, noncanonical features, stale
observations, invalid expiry, zero revisions, and out-of-range scores.

Lifecycle operations return a new checked revision. Quarantine release requires a later review;
expiry and supersession are explicit. Forgetting does not retain content in an inactive record: it
produces a tombstone binding the memory identity, prior digest/revision, deletion observation, and
reason. During replay or rebuild, a tombstone dominates records at or below its revision.

## Retrieval and rebuildable indexes

Retrieval filters before ranking. Scope compatibility, role visibility, lifecycle, quarantine,
expiry, minimum confidence, claim type, required features, and tombstones are all checked before a
candidate can receive a score.

Ranking uses bounded integer components for scope specificity, relevance, confidence, evidence
balance, recency, and feedback. Stable identity order breaks ties. Result and token limits are
checked during admission. The retrieval plan includes an explanation for every input record:
selected with component scores, or excluded with a typed reason.

Selected memories materialize as quoted, non-authoritative evidence carrying the original source
provenance. D0 may turn that metadata into a checked context node; it cannot turn it into policy or
a capability.

The memory index is a rebuildable projection over canonical records and tombstones. Rebuilding the
same ordered inputs produces the same active record set, posting lists, and digest. Correctness is
defined by the canonical active-record view rather than by an index backend, allowing future C0
storage changes without changing retrieval semantics.

## Verification and maintenance

`peritus-role` uses verification class `V`; `peritus-context` and `peritus-memory` use class `H`
because their ordinary-safe boundaries compute canonical SHA-256 content and index digests through
the existing H-class codec. Deterministic constructors, graph validation, token accounting,
selection, compaction admission, lifecycle transitions, tombstone dominance, scoring, retrieval,
and rebuild calculations remain ordinary safe Rust inside Verus boundaries. There are no trusted
constructs, exclusions, unsafe blocks, effect handles, provider calls, or placeholder success
paths.

Focused development checks are:

```text
CARGO_BUILD_JOBS=2 cargo test -p peritus-role -p peritus-context -p peritus-memory \
  --all-targets --all-features --locked
CARGO_BUILD_JOBS=2 cargo clippy -p peritus-role -p peritus-context -p peritus-memory \
  --all-targets --all-features --locked -- -D warnings
CARGO_BUILD_JOBS=2 RUSTDOCFLAGS='-D warnings' cargo doc \
  -p peritus-role -p peritus-context -p peritus-memory --all-features --no-deps --locked
CARGO_BUILD_JOBS=1 cargo verus verify \
  --package peritus-role --package peritus-context --package peritus-memory \
  --all-features --locked --check-toolchain --fwd-verus-args-to roots \
  -- --no-cheating --rlimit 20
```

The complete merge authority remains `just gate-a` plus required hosted Ubuntu, macOS, and Windows
checks. Source-layout policy keeps crate roots below 80 lines and rejects source files above the
hard 700-line limit.

## Next boundary

C6 does not run an agent. Once C6 is merged, D0 can build the durable model/tool loop by combining
B0 lifecycle transitions, B1 authority, B3/C0 durability, C4 tools, C5 providers, and C6 plans.
Review finding lifecycle and quorum adjudication remain D2; context merely supplies the fresh,
bounded evidence view they require.
