# Stage 2 domain invalidation and knowledge pilots

Baseline source: `d106c9b13318fbcdd3d816ed63ae48532806fa7e`. These standalone consumers call
ordinary public library APIs and retain their independent models in the discovery branch.
They add no production bypass, test hook, or resurrected test package.

## Working invalidation target

`invalidation.rs` exercises `WorkingState`, observation ingestion, `WorkingEvent::HostDelta`,
`HostInvalidate`, `refresh_working_state`, complete role rendering, and actual public
`encode_working_state`/`decode_working_state` snapshots.

Focused implementation and error-path inspection (not a full source audit):

- `crates/orchestration/peritus-context/src/working/{binding,state,state_access,entry,evidence,limits,validity,event,host,delta,delta_reducer,validation,staleness,invalidation,selection,error}.rs`.
- `working/wire/{mod,state}.rs` for the real versioned snapshot validation boundary.
- `crates/app/peritus-product-runner/src/local_context/{port.rs,tools/update.rs}` and
  `memory/dependencies.rs` for current host-request dependency capture, proposed validity,
  HostDelta and conservative invalidation callers.

The independent model stores captured file/content/conversation facts, source IDs, exact
fixed-length content, sticky stale status, and source frontiers. It rebuilds reverse adjacency
from the case graph and runs breadth-first reachability from invalid roots. It does not call
production validity checks, graph models, fixed-point passes, `verified.rs`, or `model.rs`.
Same-size file digest changes and missing files invalidate exactly the records that captured
them and their transitive dependents. Unrelated file changes preserve unaffected claims.
Changing an environment back does not resurrect stale conclusions. A new observation alone
does not declare an old conclusion refreshed; a fresh source-backed delta is necessary.

Initial records use ordinary validity declarations under a host-bound delta. Later repairs
capture current facts and ingest fresh observations; host-invalidated records retain unknown
validity until a fresh checked proposal replaces them. The model preserves the exact rule
that directly invalid records refresh their stale frontier during reduction, whereas an already
stale record whose own facts hold does not continually advance solely because a dependency
remains stale. This distinction is checked at the public entry projection.

Generated DAGs include forward-ID dependencies, so multiple fixed-point passes are reachable.
Operations warm/read, mutate a file, remove/restore it, change candidate/conversation, repair,
invalidate, reject stale-source repair, and reopen. Rejected run/workspace/task/role scopes,
state revisions, backwards conversations and conflicting observation identity are followed by
independent state checks and complete reads. Every operation remains explicit in persisted Case
JSON; operand resolution comes from logical case/model state, never a production lookup result.

The fixed canary uses a reverse-ordered three-node chain with a file-bound terminal prerequisite.
`calibration/stage2-invalidation.patch` limits the real invalidator to one pass. A file change
then leaves earlier dependent records current, and the external BFS model must reject that
state. It is a plausible missing-fixed-point repair, not an oracle fault or compiler error.

Metrics distinguish file/candidate/conversation changes, same-size changes, new observations,
direct and transitive invalidations, fresh repairs, restored-current records, each rejection
family, actual wire reopen, and reviewer exclusion. Counters are observations of exercised
behavior, not branch-coverage percentages or proof of no remaining defects.

## Run-knowledge target

`knowledge.rs` calls checked `KnowledgeBinding`, `KnowledgeSection`, `RunKnowledgeSnapshot`,
`CurrentKnowledgeState`, `InvalidationRequest`, `plan_invalidation`, and `plan_delta_packet`.

Focused implementation and caller inspection (not a full source audit):

- `crates/orchestration/peritus-run-knowledge/src/{binding,change,section,snapshot,source,kind,limits,error,plan,delta}.rs`.
- `snapshot/validation.rs`, `plan/evaluation.rs`, `delta/{material,packet}.rs`.
- `crates/orchestration/peritus-run-settlement/src/identity.rs` for the exact candidate axes.
- `crates/app/peritus-product-runner/src/execution/resume/knowledge.rs`: production capture
  creates separate writer/reviewer/fixer snapshots and plans the resume phase from reuse.

The model uses raw candidate fields, a small source map, section content labels, declared kinds,
and adjacency lists. It independently evaluates documented reason precedence as a declarative
table, then finds all transitively invalidated consumers through reverse edges. The oracle
compares every section decision and all plan accounting. Candidate comparisons are ordinary
field comparisons; no production candidate equality or knowledge model helper is the oracle.

Candidate dimensions include run, workspace, source content, repository snapshot, optional
execution context, incorporated conversation, and checkpoint/creation sequence. Kind-specific
conversation/candidate sensitivity and clarification eligibility are explicit input-level tables.
Source changes and omissions are checked even when the declared change class is provider failure
or same revision. Source zero is a permanent nonempty catalog anchor; other source IDs can be
removed, restored, shared, or changed without changing their byte length.

Fresh snapshot capture builds a delta packet against the prior snapshot. The oracle checks
navigation-only kinds stay navigation; exact unchanged current facts become references; changed
or invalidated facts are redelivered. It compares complete source bindings and dependency lists
as well as material content. A source absent from the new capture is omitted from that section's
binding; an otherwise source-free section binds the permanent anchor, so all generated checked
snapshots retain nonempty valid provenance.

The constructor rejects cross-role bindings; the delta API rejects cross-role snapshots and a
current snapshot carrying stale sources. Invalid clarification targets are rejected, then a
valid request continues against the unchanged retained snapshot. `RoleIsolation` in the planner
itself is unreachable through valid public snapshot constructors, so construction and delta
rejection are the executable isolation boundaries tested here.

There is no public run-knowledge durable snapshot codec in this source surface. The pilot
reconstructs through checked public constructors and compares exact snapshots; it does not call
that a storage reopen. The working invalidation pilot above does exercise an actual wire reopen.

The fixed knowledge canary changes source ID 1 in a file-map section, whose design and evidence
consumers have different direct source bindings. `calibration/knowledge.patch` suppresses real
dependency invalidation. The independently recomputed consumer set must fail at the source
change, before any later snapshot refresh. Navigation-only packet classification and every
candidate axis also have fixed examples alongside generated operation histories.

## Verification and limits

The parent runner owns final persisted campaign evidence, serial Cargo allocation, and isolated
canary application/reversal. Do not interpret source presence as a completed campaign. Local
qualification consumer files in this worker are temporary integration aids; only the two
modules, this contract, and the two calibration patch files are handed to the central runner.

Worker qualification used an isolated nested consumer: offline single-job Cargo build, all four
fixed examples per module, then 512 generated histories per module with seed `20261030` and
maximum 48 operations passed. A subsequent strict offline Clippy pass passed with `-D warnings`.
The central runner must repeat its campaigns after integration and record final source digests.
Both calibration patches pass `git apply --check`; deliberate mutation execution remains with
the central isolated calibration runner. Baseline witnesses reach two transitive invalidations
in each domain. These execution facts do not imply completed native or end-to-end qualification.

These models establish bounded pure-domain behavior and real public codecs. They do not certify
filesystem observation correctness, archive/artifact existence, product provider request assembly,
host authority enforcement, concurrent mutation admission, durable process crash recovery, or
live provider costs. SHA-256 helpers construct valid digest-bound inputs; hash correctness is
not the oracle. No production mutation or fix is part of these deliverables.
