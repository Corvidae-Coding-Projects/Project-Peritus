# Daily-driver state ownership and recovery

## Required outcome

An installed Peritus session preserves the user's intent and useful work through ordinary
interruptions. Every displayed fact has an authoritative owner. Unknown outcomes remain unknown;
each recoverable block has a route through the normal interface. This work is not complete merely
because individual tests, benchmarks, or a design document pass.

The user requires four architectural priorities: one owner per important fact, content separated
from execution context with explicit evidence dependencies, runtime and startup sharing recovery
rules with stable operation identities, and host observations independent of model judgments.

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

Next, introduce candidate content and execution-context identities at the host observation
boundary. Evidence declares its dependencies; legacy records conservatively depend on the full
snapshot they actually observed. Git-dependent gates and reviews must not be relabeled
content-only. Include conversation requirements and declared environment/toolchain inputs in
dependency binding. Newly observed metadata must not be invented for old records.

Expose operation knowledge, uncertainty, and legal recovery from the authoritative domains.
Startup and runtime use the same reconciliation methods. Retry means resuming the original
operation, not admitting an indistinguishable new effect. Unknown external outcomes require
inspection/reconciliation; permission to retry does not prove the original command did nothing.

Finally, retain host tool observations independently of final model-report parsing and independently
of `RunState` creation. A model report supplies a summary and proposed run instructions. Its absence
does not erase observed changes or exit statuses and does not authorize a guessed run command.

## Alternative and compatibility

A new universal event store could replace control, run records, and command receipts at once.
It would also require migrating independently persisted state and every client before the first
usable increment. Prefer consolidating each fact in its existing owner with explicit migrations.

The first reconciliation increment keeps all persisted and wire fields and tags unchanged. It
permits lower qualification in a later observation while retaining strict evidence construction,
lineage, sequence, and exactly-once terminal settlement. Old readers already decode lower stages.
New content/context metadata and recovery capabilities will require versioned persistence and
negotiated protocol additions, including CLI, TUI, WebUI, and generated schemas/fixtures. Older
clients must receive conservative compatible projections or a clear unsupported-capability result.
Rollback must never reinterpret an unknown effect as safe to rerun.

## Delivery and completion evidence

1. Centralize evidence freshness and supported qualification; reproduce unchanged-content
   requalification failure, check domain refinement, preserve restart and commit behavior.
2. Migrate candidate observations and explicit evidence dependencies through persistence,
   continuations, negotiated transport, and normal interface. Exercise content edits, Git-only
   commits, nested repositories, modes, symlinks, requirement changes, and environment changes.
3. Publish authoritative operation/recovery projections and connect ordinary recovery controls.
   Exercise admission-before-disconnect, exact retry, cancel, daemon kill, provider loss,
   command uncertainty, and restoration of unavailable workspace/state storage.
4. Retain host facts across missing/malformed model reports and process restart, with honest
   qualification and ordinary inspection/export/continuation paths.
5. Qualify the installed artifact on persistent, real daily jobs. Keep one installation and its
   history across days. Interrupt real workflows at effect boundaries. Compare intent, actual
   files/effects, displayed state, original operation identity, and available recovery actions.

Any need to edit JSON/SQLite, invent a restart sequence, reconstruct a vanished draft, or blindly
repeat an uncertain effect fails the user-flow acceptance gate. Automated regression and formal
checks support the observations; their counts do not establish daily-driver viability.

## Status

Implementation and qualification are in progress under issue 99 and PR 105. The four-priority
goal remains active until all delivery and ordinary-interface recovery evidence is present.
