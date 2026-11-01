# Stage 4 scheduler and collaboration discovery

Final bounded qualification is recorded in `../STAGE4-REPORT.md` and `../inventory/stage4-targets.json`; earlier worker-status notes describe their historical snapshot.

Baseline `d106c9b13318fbcdd3d816ed63ae48532806fa7e`, with the coordinating Stage 3 discovery
overlay. The two ordinary library consumers live in `src/stage4/{scheduler,collaboration}.rs`.
They retain explicit operation cases and independent state models. Production selectors, reducers,
replay routines and verified predicates never supply expected outcomes. Public replay is itself
a subject, and its result is checked against independently accepted history and full state.

## Source and caller coverage

This is a bounded owner-mechanism pilot, not complete line or package coverage. Read active
manifests, public exports and the nearest package/source/reducer/selection navigation maps.

| Owner | Implementation read for this pilot | Current caller read |
| --- | --- | --- |
| Scheduler definitions | `binding.rs`, `limits.rs`, `semantics.rs`, `command.rs`, `command/kind.rs`, `resource.rs`, `work.rs`, `work/{class,phase,record,terminal}.rs`, `worker.rs`, reservation accessors in `worker/clone_impl.rs` | `peritus-daemon/src/domain/dispatch.rs::scheduler`; native `rsi/scheduling.rs::begin` |
| Scheduler transitions and selection | `reducer.rs`; admission/checks, dispatch, worker-control and control phase/cancellation branches; `selection.rs`, `selection/{scan,scan/work,capacity}.rs`; dependency action/round application; start, completion and cancellation acknowledgement kernels; queue pressure and worker refresh | The daemon uses one serialized `SchedulerSession`, then `start`/`decide` and guarded session commit. Native rollout dispatch independently checks the selected exact work/worker pair. |
| Scheduler durable boundary | `session.rs`; public commit/load sections of `durability.rs`; state, work and reservation getters; `runtime.rs` pending directives | This campaign reproduces that public session/commit shape against a real temporary SQLite file. |
| Collaboration | `binding.rs`, `limits.rs`, `command.rs`, `task.rs`, `join.rs`, `message.rs`, `reducer.rs`, `reducer/apply.rs`, `reducer/apply/terminal.rs`, `state.rs`, `state/{mutation,validation}.rs`, `replay.rs`; public commit/load sections of `durability.rs` | `peritus-daemon/src/domain/dispatch.rs::collaboration` loads/rebuilds the aggregate, reduces, and commits its exact successor. |

Some scheduler support files were inspected as focused function slices rather than complete
files. Formal/verified predicates and production selection models are not reference oracles.
These journal-backed domain owners remain separate from interactive `ProductRunService`.

## Scheduler contract and independent model

A small ordinary map retains immutable jobs, dependency/parent links, enqueue order, attempts,
retry causes and terminal outcomes. A second map records exact dispatch ownership and requested
resources; an independent permanent set prevents reuse after release. Every selection enumerates
all work/worker pairs and filters them by ownership, execution class, availability, concurrency,
global capacity and local capacity. The oracle chooses the minimum documented tuple: aged first,
higher priority, earlier admission, work identity, worker identity. It does not call production
feasibility or ranking helpers. A configured active-reservation ceiling is separately enforced at
dispatch admission; the read-only selector's contract is feasible pairs, including during pause.

Resource totals are fully recomputed with `u64` sums over small positive two-dimensional requests.
Raw canonical entries and every public immutable work/worker/reservation field are compared with
case values. Constructor-created production objects are inputs, not the expected field values.
Bypasses advance only for work independently found feasible immediately before dispatch. Waiting
dependencies reach a fixed point of readiness or the first canonical failed prerequisite.

Histories exercise admission, dispatch, start acknowledgement, success, each explicit failure
disposition, bounded retry, direct/tree cancellation and owner acknowledgement, worker drain and
availability restoration, pause/resume, stale fences, reads, exact latest-command receipt replay,
and SQLite close/reopen. `ExternalToggle` commits through the public non-session API after warming
the real session, so the next session read must invalidate and replay. Rejected operations retain
the exact accepted state; `accepted_after_rejection` counts an actual later accepted mutation.

For `max_steps >= 32`, the generator weights broad mixed histories, owner-sensitive dispatch,
aging and worker loss as 2:1:1:1. Shorter bounds omit the loss family, and very short bounds use
broad histories only. Owner-sensitive histories reach successful completion or rejection → retry
→ new reservation → cancellation. Aging histories construct higher-priority traffic until a
lower-priority item reaches its threshold, then require it to precede further higher-priority work.
Worker-loss histories vary recovery policy, prior start and cancellation with mixed continuations.
Configurations and suffixes vary; these are generated interaction families, not just fixed cases.

`LoseWorker` removes every reservation independently owned by the lost worker, conserves other
workers' exact reservations and resources, and records the exact ordered public outcome list.
RetrySafe work requeues below the attempt cap and becomes exhausted at the cap; Ambiguous and
Fail policies terminate with their distinct truthful classifications; cancelling work becomes
cancelled. Dropping/reopening the real SQLite owner preserves those outcomes. Late start, finish
and cancellation acknowledgements for released dispatch identities reject; the permanent identity
set prevents reuse. Explicit availability restoration makes a lost worker eligible again, and
continued new work must still dispatch and finish. Loss counters distinguish real affected-work
outcomes from empty worker-loss events.

The pilot bounds three workers, two owners plus rejected unknown owners, five classes, two
resource dimensions, twelve queue-pressure slots, twenty-four retained jobs, at most four live
reservations and three attempts. It targets current `StrictRecoveryQueueV2`, not legacy schema 1.
Worker removal, explicit abandonment/exhaustion commands and final scheduler aggregate
classification, arithmetic at `u64::MAX`, configurable byte exhaustion, corrupted wire imports and
arbitrary concurrent writers are not qualified here. Attempt exhaustion caused by worker loss is
covered above; this does not establish every explicit terminal command. The late-start-after-cancellation ordering question is tracked separately; this
pilot does not manufacture a stronger start-acknowledgement rejection contract.

`calibration/stage4-scheduler.patch` removes worker-owner matching from the real feasibility
predicate. The generated owner-sensitive family admits owner-one work while canonical worker
zero belongs to owner zero. The independent feasible-pair query must reject that wrong selection
before it could become a reservation. Fixed and fresh generated searches must detect the mutation.

## Collaboration contract and independent model

An ordinary parent map retains tasks, exact owning actor, required/optional status, join policy,
depth, accepted/active/cancelling/terminal phase and inert reservation observation. Descendants
are recomputed by breadth-first expansion of child links. Required-child outcomes are evaluated
as an independent all/any/none truth table. Successful completion cannot bypass a required join;
pending delivery/cancellation or a nonterminal task prevents aggregate finalization. Cancellation
from a task or ancestor owner reaches every nonterminal descendant. Active tasks require explicit
termination acknowledgements; inactive tasks become cancelled immediately.

Each task has an independent chronological message-ID list, separate from the canonical identity
map. Expected ordinals and predecessors follow this causal list. Every public message field,
delivery acknowledgement, artifact/evidence handoff and immutable assignment is compared with
known logical values. The oracle checks descendant and child projections, join decisions,
pending-directive classification and exact aggregate terminal/blocker truth after every step.

Broad histories cover offers, accept/reject, activation, bidirectional owner/parent messages,
wrong owners/bindings, acknowledgement, positive/negative completion, abandonment, subtree
cancellation, pause/resume, finalization, stale fences, latest-command replay and SQLite reopen.
Fixed cases include all six message-ID permutations, interleaved tasks and acknowledgements,
stale predecessor rejection followed by a valid message, empty all/any joins, required-child
failure, optional descendants, payload `1024/1025`, recipient bounds and message/completion artifact
limits. Half the generator is broad histories. One quarter constructs root/child causal streams
with independently selected ID permutations, direction and acknowledgement/reopen interleavings;
some retain active tasks for further mutations. One quarter constructs a three-level subtree and
ancestor cancellation, with required/optional and all/any join variations plus a generated suffix.

Reservation observations are public inert data. This pilot does not claim that a lower scheduler
or OS effect really ran for each observation; the exact scheduler-to-collaboration integration
belongs to a separate owner campaign. Likewise, handoffs are exact domain metadata rather than
filesystem artifact reads. Limits are small: twenty-four tasks, depth/fan-out at most four,
thirty-two messages, one/two recipients and at most four artifact references. Arbitrary import
corruption, broad nominal/text constructor fuzzing, full wire compatibility, disk-full and process
crashes mid-transaction, multiple writers and daemon authorization are outside this pilot.

`calibration/stage4-collaboration.patch` omits descendants from real cancellation propagation.
The generated subtree family activates a child and grandchild, cancels the child from the root
owner, then inspects both phases before acknowledging each. The independent BFS model requires
both active tasks to be Cancelling. Fresh generated search must reach and detect this mutation.

## Confirmed causal predecessor defect and evidence boundaries

On original production source, a real public reducer/SQLite consumer accepted ordinal-one message
ID 30, accepted ordinal-two ID 20 with predecessor 30, and reopened the journal after each. The
legitimate ordinal-three ID 10 with predecessor 20 was then rejected as `CausalityViolation`.
Increasing IDs `10 → 20 → 30` all succeeded. Before/after source hashes were identical during this
observational probe; raw command, output and status are in lane `stage4-probe/evidence/descending-02`.

The source chose the preceding message from the identity-sorted map's last record. Causal order
is a distinct ordinal relation. A separate focused branch `fix/collaboration-causal-predecessor`
contains a readable deterministic public-API example and the minimal maximum-ordinal correction.
The regression fails before the fix and passes after; exact raw logs, source hashes and patch are
in that worktree's `discovery/proptest-praxis/stage4-regression/evidence`. The broad discovery family
retains descending and mixed permutations plus independent task streams and stale-predecessor
rejections. Candidate SDK qualification must exercise the focused correction; it must not weaken
the model to accept identity-dependent causal order.

Formatting and patch applicability do not establish a passing pilot or successful calibration.
The coordinating runner owns final source fingerprints, compilation, seeded searches, observed
interaction gates, deliberate baseline/mutant/restored cycles and exact replay evidence. Original
failed/incomplete attempts remain separate historical evidence.

## Integrated evidence status

All twelve Stage 4 targets, including these two consumers, are integrated. Final qualification is
pending: source implementation and earlier native QA are not a final-source seal. The coverage
ledger retains historical scheduler/collaboration reports and observed counters separately from
pending final fixed/generated counts and controlled mutation-cycle selection.
