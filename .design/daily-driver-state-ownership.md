# Daily-driver state ownership and recovery

## Required outcome

An installed Peritus session preserves the user's intent and useful work through ordinary
interruptions. Every displayed fact has an authoritative owner. Unknown outcomes remain unknown;
each recoverable block has a route through the normal interface. This work is not complete merely
because individual tests, benchmarks, or a design document pass.

The user requires four architectural priorities: one owner per important fact, content separated
from execution context with explicit evidence dependencies, runtime and startup sharing recovery
rules with stable operation identities, and host observations independent of model judgments.
There are no existing users or released histories to preserve. Backward compatibility with earlier
product state, request shapes, or UI flows is explicitly out of scope. Old state is retained only as
quarantined evidence; it is never admitted through compatibility defaults, upgraded, or executed.

## Inspected boundaries and current findings

| Fact | Current owner or source | Consumer and mismatch |
| --- | --- | --- |
| Durable user input and operation admission | Product-control store and runner `control` domain | Workbench execution and TUI receipts must distinguish admission from worker progress. |
| Workspace observation | Runner `candidate`, `progress`, and execution checkpoint recorder | `candidate_digest` includes source and Git history; `candidate_source_digest` separately excludes history. Neither is a complete environment identity. |
| Evidence freshness and qualification | Settlement checkpoint constructor plus three separate host implementations | Recorder `carry`, daemon recovery `stale`, and commit handoff duplicate freshness. Stage advancement rejects a newer failed check on unchanged files. |
| Terminal attempt disposition | Settlement reducer; daemon `product_run/execution.rs` projects it | Internal automated `Accepted` disposition becomes public “Accepted” before a human accepts the deliverable. |
| Human handoff decision | Deliverable `accepted`, `commit_revision`, and `discarded` | Runs rendering must use these facts, independently of automated qualification and attempt phase. |
| Command outcome | Developer effect receipts and command kernel/journal | Handle JSON is a projection. Restart currently converts a projected “running” label to indeterminate without exposing command reconciliation through the TUI. |
| Host-observed successful commands | Developer-tool executor | `turn.rs` accumulates results before parsing the final model report, but an error return bypasses `AppliedWrite`; finalization without `RunState` loses those command observations. |
| Candidate recovery after interruption | Durable run record and daemon recovery | Startup converts interrupted phases and reconciles terminal handoffs; these must converge with live observation and explicit recovery. |

## DRY and YAGNI audit

The current product layer still represents two generations of the same concept. A product run can
exist without workbench ownership, `ProductInteractionBinding` makes its conversation optional for
legacy runs, `PersistedRecord` makes interaction optional and encodes three meanings into numeric
phase offsets, startup has separate product-run and workbench loaders, and the application protocol
retains legacy snapshot and deliverable interpretations. A version-2 workbench record can also omit
the `goal_resume` field and acquire `None` during deserialization; a test explicitly treated that
shape as compatible after the field was introduced.

This is semantic duplication, not merely repeated syntax. Admission, observation, persistence,
recovery, and the TUI can disagree because each supports a different subset of the run ontology.
DRY therefore means one owner and one transition definition for each fact. It does not mean hiding
different facts behind a generic state helper. YAGNI means deleting the unused generation and its
fallbacks rather than adding adapters between old and new shapes.

The retained product has one durable workbench-run aggregate. Every run has an exact conversation,
start operation, execution attempt, candidate identity, and recovery projection. Absence is valid
only where it is a current domain fact, such as no candidate before any content exists; omission of
a required serialized field is invalid. Optional facts may use the one representation defined by
the current format. Live protocol negotiation and record version tags remain as fail-fast integrity
boundaries, but they do not authorize compatibility decoding between format versions.

This is a boundary map, not a whole-codebase coverage claim. Commands and paths were inspected
in the repair worktree. Large files read in sections remain partial coverage in the evidence log.

## Preferred design

Keep separate facts for durable intent, operation admission/effect/outcome, current attempt,
candidate material, qualification observations, and human handoff. The existing control, receipt,
and settlement domains should own their respective facts; add missing facts there rather than
building a second UI or daemon state machine. Projections carry those facts and legal actions.

First, the settlement domain owns reconciliation of evidence and the qualification currently
supported. Observation sequence is monotone; qualification strength is not. A newer failed
observation on identical files must produce an accessible unqualified candidate, not an invariant
error or a retained success claim. Retained evidence keeps its producing identity.

Next, keep candidate content, repository context, requirements, and execution context as separate
host observations. Evidence declares its dependencies. Deterministic gates bind content,
requirements, and the execution context; obligations and review bind content and requirements.
Repository context fences handoff operations without invalidating source-bound evidence. A daemon
restart clears process-local execution context and retains a continuation that reacquires only the
stale gates.

Expose operation knowledge, uncertainty, and legal recovery from the authoritative domains.
Startup and runtime use the same reconciliation methods. Retry means resuming the original
operation, not admitting an indistinguishable new effect. Unknown external outcomes require
inspection/reconciliation; permission to retry does not prove the original command did nothing.

Finally, retain host tool observations independently of final model-report parsing and independently
of `RunState` creation. A model report supplies a summary and proposed run instructions. Its absence
does not erase observed changes or exit statuses and does not authorize a guessed run command.

## Alternative and canonical state

A new universal event store could replace control, run records, and command receipts at once, but
that would add a second source of truth before it removed the first. Consolidate each fact in its
existing owner and keep one canonical projection.

Replace the parallel legacy and workbench product paths with one current record and one public
admission, observation, and recovery protocol. Bump the record and application protocol at the cut;
do not preserve old tags, missing-field defaults, optional legacy bindings, dual loaders, or legacy
UI actions. Earlier records are quarantined rather than guessed or upgraded. CLI, TUI, WebUI,
daemon, generated schemas, fixtures, and documentation move together. Rollback must never
reinterpret an unknown effect as safe to rerun.

## Delivery and completion evidence

1. Centralize evidence freshness and supported qualification; reproduce unchanged-content
   requalification failure, check domain refinement, preserve restart and commit behavior.
2. Carry candidate observations and explicit evidence dependencies through persistence,
   continuations, negotiated transport, and normal interface. Exercise content edits, Git-only
   commits, nested repositories, modes, symlinks, requirement changes, and environment changes.
3. Publish authoritative operation/recovery projections and connect ordinary recovery controls.
   Exercise admission-before-disconnect, exact retry, cancel, daemon kill, provider loss,
   command uncertainty, and restoration of unavailable workspace/state storage.
4. Retain host facts across missing/malformed model reports and process restart, with honest
   qualification and ordinary inspection/export/continuation paths.
5. Remove the parallel legacy product generation. Require the current record shape, exact
   conversation and start ownership, one loader, one observation projection, and one set of normal
   recovery controls. Quarantine every older or incomplete record without compatibility decoding.
6. Qualify the installed artifact on persistent, real daily jobs. Keep one installation and its
   history across days. Interrupt real workflows at effect boundaries. Compare intent, actual
   files/effects, displayed state, original operation identity, and available recovery actions.

Any need to edit JSON/SQLite, invent a restart sequence, reconstruct a vanished draft, or blindly
repeat an uncertain effect fails the user-flow acceptance gate. Automated regression and formal
checks support the observations; their counts do not establish daily-driver viability.

## Status

Implementation and qualification are in progress under issue 99 and PR 105. The four-priority
goal remains active until all delivery and ordinary-interface recovery evidence is present.

Delivery items 1 through 5 are implemented on the PR branch. Product-run records require format 6
and reject every earlier shape without migration. Current clients and the daemon negotiate
application protocol 2.0; protocol 1.0, direct product start, continuation-only requests, run-local
conversation ownership, and ownerless interaction records are gone. `/build` and ordinary
conversation enter through one required durable workbench conversation and input ledger. Candidate
content, repository context, requirements, and execution context remain distinct observations with
explicit evidence dependencies. Host command facts survive a missing or malformed model report.

A real PTY probe against a separately running daemon and a declared writable folder exercised the
ordinary interface without a model provider. `/sessions new Real PTY smoke` moved from an explicit
unconfirmed state to durable revision 1, appeared in `/sessions`, and remained discoverable after
both the TUI and daemon restarted on the same state directory. This establishes process-boundary
admission, projection, and restart persistence for that path; it is not evidence for provider
execution, interruption during external effects, or multi-day installed use.

The WebUI uses the same durable create, queue, and start controls. Its browser tab,
conversation, and run identities are separate required facts; controls and exact-run CLI handoffs
use the run, while workbench operations use the conversation/workspace query. Before the first
mutating daemon request, the gateway retains the exact prepared execution context independently of
the mutable browser draft and current settings. Each stage has a deterministic identity and uses
the daemon's authoritative receipt query after interruption. A definitively accepted queue whose
execution cannot be confirmed is displayed as durable and pending with an ordinary Workbench route,
rather than as a total send failure that invites a duplicate message. Earlier or incomplete WebUI
state is quarantined; there is no compatibility decoder or missing-field default for the cut. Run
snapshots now require an explicit authoritative operation projection at construction. The browser
uses that projection for activity, facts, uncertainty, and legal actions; it no longer infers a
`busy` fact from the phase. Unknown command outcomes expose ordinary acknowledgement and exact
retry controls without inviting replay. A queued message remains durable but cannot start a new
provider request while command, commit, or discard ownership is unresolved; continuation becomes
admissible only when the authoritative execution projection says it is at a recoverable boundary.
`/doctor` reads that same projection and distinguishes live work from operations that require
explicit reconciliation instead of deriving a reassuring answer from the broad lifecycle phase.

The real browser/gateway suite exercised 24 HTTP, persistence, restart, auth, recovery-banner, Git,
file, PDF, and exact-PTY flows against freshly built binaries. The first run exposed a stale-binary
precondition and hard-coded `/tmp/opencode` fixture roots; the workflow now builds the exact binary
and uses the operating system's temporary directory. This is process-boundary evidence for the
gateway and ordinary recovery UI, not provider-execution or multi-day evidence.

The performance qualification probe and the internal improvement evaluator now use the durable
create, queue, and start path. Improvement evaluation persists a stable actor-owned conversation
before the first workbench mutation, separates the candidate and each evidence observation into
bounded queue inputs, and makes the evaluation directive depend on those exact inputs. Its public
projection exposes the conversation, run, and target workspace even when execution admission has
not completed, and the WebUI can open that durable workbench directly. The old improvement request's
ignored task field is gone, and pre-release improvement schema 1 is rejected rather than migrated.

No production caller creates a new interaction or product snapshot without explicit workbench and
operation ownership. Startup and live queries converge on the same operation projector, and the
daemon retains the stable original identity when it reports recovery or unknown effects.

Delivery item 6 remains incomplete. The branch has real PTY restart and abrupt daemon termination
evidence plus real browser/gateway recovery evidence, but it has not yet qualified one installed
artifact across several days of real provider-backed work. Cross-platform descendant containment
also remains unproven outside the Linux parent-death mechanism. Daily-driver viability therefore
remains an active qualification claim rather than a completed result.
