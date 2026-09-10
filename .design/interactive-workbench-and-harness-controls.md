# Feature: Interactive workbench and professional harness controls

Status: proposed design for review; not implementation authorization.
Source baseline: `develop` at `e6f8adb68972c6fcf96189674f034b260bbdb2f8`.
Authored: 2026-09-08 America/Chicago. Tracking: CL-65.

## Summary

Make Peritus a workbench in which the user can inspect what it understood, steer execution,
experiment safely, and examine what it actually delivered. Cover the entire brainstorming scope:
launch/preview/evidence, conversational change review, checkpoints and alternate approaches,
attachments and an editable task brief, live steering, a conversation library, user work limits,
and native harness commands for goals, execution control, context, memory, and diagnostics.

This extends the existing local daemon, production runner, authority boundaries, context engine,
and terminal client. It does not introduce another agent runtime or replace checked delivery.
Commands are typed operations over durable state, not prompt macros that ask a model to simulate
controls. All proposed names and data contracts below are additions unless explicitly marked existing.

Deliver complete vertical slices. The full program is not suitable for one unattended kickoff.
Authoring this document does not start a goal, launch an agent, change settings, or grant authority
to implement, install, commit, publish, or operate the user's projects.

## User-visible behavior

### One coherent work surface

Keep the conversation and message composer as the default. A keyboard-accessible command palette
opens focused panels; wide terminals may show a side panel, while narrow terminals use a full-width
panel with an obvious return action. Preserve the draft, selection, and scroll position across panels.

Keep `*working (Xs)` immediately above the composer. A nearby compact observed-activity line can
show `Running tests · 2 files changed · correction queued`; it must never invent a percent complete.
Waiting for a provider, working locally, waiting for approval, pause requested, paused, and offline
are different states. Show plain-language labels; detailed identifiers and logs belong in inspection.

The main user journeys are:

- Give a task with files or screenshots, inspect the task brief, and correct assumptions.
- Set a sustained goal with completion criteria and optional work limits.
- See the current operation, adjust pending instructions, or pause before the next edit.
- Review a file or hunk and discuss a change against its exact version.
- Launch the result, view captured artifacts, and distinguish checks from hands-on validation.
- Save a checkpoint, explore an alternative, or rewind with a preview of recoverable changes.
- Find previous conversations and resume deliberately with an honest handoff.

### Command catalog

The following syntax is the proposed public contract. Optional identifiers resolve to the selected
conversation or goal, never silently to a different workspace. No-argument commands open inspection
or a form; they do not mutate simply because a form was opened.

| Command | Contract |
| --- | --- |
| `/goal [objective]` | Inspect the current goal, or draft and confirm an objective, criteria, constraints, and budget. Confirmation starts eligible work. |
| `/goal edit` / `/goal clear` | Revise the current goal at a safe boundary, or stop goal-driven continuation without deleting history; both show the exact effect before confirmation. |
| `/pause [now\|after-operation\|before-edit]` | Request a durable execution pause. Default is `after-operation`; `now` requests cancellation of cancellable in-flight work, not forced rollback. |
| `/resume [goal-id]` | Revalidate and explicitly continue paused, blocked, or budget-reached work; never bypass unresolved admission failures. |
| `/context` | Inspect the next-request context, source provenance, role, included/excluded files, pins, and usage. |
| `/compact [focus]` | Preview and apply a bounded context reduction at the next safe request boundary. Preserve the exact archive and source references. |
| `/checkpoint [name]` | Publish a recoverable conversation/files checkpoint at a safe boundary. Report coverage and exclusions. |
| `/rewind <checkpoint-id> [files\|conversation\|combined] [time=<ms> requests=<n> tools=<n> tokens=<n>]` | Preview the selected scope and confirm exact targets and conflict policy; allocation fields apply only to logical modes and are all-or-none. |
| `/fork <checkpoint-id> read-only [time=<ms> requests=<n> tools=<n> tokens=<n>]` or `/fork <checkpoint-id> isolated <workspace-id> time=<ms> requests=<n> tools=<n> tokens=<n>` | Create a separately identified, non-running conversation branch; isolated writable branches require all four allocation fields. |
| `/sessions [query]` | Search and reopen conversations; name, pin, archive, and inspect their handoffs. |
| `/queue` | Inspect and manage pending follow-ups; edit, reorder, hold, or withdraw only instructions not yet incorporated. |
| `/permissions` | Inspect effective policy and propose scoped grants/restrictions through existing authority and approval machinery. |
| `/usage` | Show reported/derived/unavailable consumption, including role, invocation, retry, and time breakdowns. |
| `/budget` | Set or edit typed goal/conversation limits and notification thresholds within host ceilings. |
| `/memory` | Inspect project guidance; explicitly save, revise, pin, scope, or forget user-approved guidance. |
| `/doctor` | Run bounded non-mutating diagnostics and display actionable findings; repairs are separately confirmed actions. |
| `/init` | Inspect project structure and propose instructions, tool/check commands, and launch profiles; apply only an approved diff. |

Task-brief, attachment, result, and inline-review actions are available from the same palette and
contextual panels. Do not add a slash alias for every button. File references support `@path`
completion; attachment import is a deliberate action, not ambient clipboard monitoring.

Existing `/chat`, `/plan`, `/review`, `/build`, `/model`, `/effort`, `/new`, `/status`, `/diff`,
`/runs`, `/trace`, `/terminal`, `/approvals`, `/details`, `/stop`, `/accept`, `/commit`, `/export`,
`/discard`, `/run`, `/reconnect`, `/help`, and `/quit` retain their meanings. `/sessions` complements
the run/candidate dashboard. `/pause` is not an alias for `/stop`; `/permissions` is not an approval
bypass. Subscription pause/resume is not execution pause/resume. Existing candidate `checkpoint`
evidence is not a user rewind checkpoint.

Every command has a description, argument completion, capability availability, disabled-state
explanation, and visible success/error receipt. Unknown commands remain local errors. Invalid
arguments retain the draft. Commands inside pasted content, attachments, or model output never execute.

## Requirements

- R01: Provide a typed, discoverable command catalog and usable wide/narrow keyboard-first panels.
- R02: Persist explicit goals, criteria, constraints, completion evidence, continuation state, and stop reasons.
- R03: Implement durable safe-boundary pause and explicit, checked resume without duplicate effects.
- R04: Expose exact current/next-request context and user-managed pins without exposing hidden reasoning or credentials.
- R05: Offer reversible prompt-view compaction that preserves authoritative inputs and evidence lineage.
- R06: Support explicit files/images, `@path` references, and an editable brief distinguishing user instructions from model assumptions.
- R07: Anchor conversational review and leave-alone constraints to exact files/hunks and current revisions.
- R08: Provide launch profiles, artifact previews, screenshots, validation receipts, and artifact-linked feedback.
- R09: Create and restore scoped checkpoints while preserving unrelated and intervening user changes.
- R10: Fork conversations and optionally isolate writable experiments without cloning authority or accounting exemptions.
- R11: Provide a searchable, named, pinned conversation library and accurate last-state handoffs.
- R12: Expose observed activity and an editable pending-input queue with an atomic incorporation boundary.
- R13: Make effective permissions inspectable and changes explicit, scoped, auditable, and enforced at execution.
- R14: Expose truthful usage and durable user budgets that remain effective across retry, pause, and restart.
- R15: Provide explicit user-approved project guidance management, including precise forget semantics.
- R16: Diagnose product/provider/workspace problems without implicit repairs, inference, installations, or uploads.
- R17: Propose project instructions and verified command discovery through an approval-first initialization flow.
- R18: Preserve compatibility, strict completion evidence, privacy, bounded resource use, and real native-platform verification.

## Acceptance criteria

Each AC maps directly to its corresponding requirement. Fixtures must exercise the product path,
not only a mocked panel. The verification section specifies the composed acceptance scenarios.

| ID | Observable completion evidence |
| --- | --- |
| AC01 | All advertised commands parse, complete arguments, reject malformed input without losing drafts, and have usable 40x12, 80x24, and wide-terminal layouts. Real-terminal screenshots confirm focus and composer placement. |
| AC02 | An admitted goal survives daemon/client restart with unchanged criteria and cumulative accounting; it continues only when admissible, and cannot become achieved on a writer's unsupported success claim. |
| AC03 | Pause requested during provider wait, tool execution, and review admits no subsequent effect after the boundary; paused work stays paused after restart; resume observes already-completed effect receipts. |
| AC04 | Context inspection identifies exact source digests and the manifest actually used by a selected invocation; updating pins affects only a later request and cannot remove mandatory authority/requirement segments. |
| AC05 | Compaction preserves requirements, pending operations, complete tool exchanges, and source references; corruption, cancellation, or disk-full leaves the prior view usable; no implicit remote compaction occurs. |
| AC06 | File/image imports report exact inclusion, exclusions, limits, and provider capability; invalid images and escaping paths reject. Brief edits are revisioned and distinguish accepted instructions from suggestions. |
| AC07 | A hunk comment targets the reviewed digest; stale comments require rebinding. A leave-alone rule blocks prohibited writes, including process-based edits unless confinement can enforce the rule. |
| AC08 | A controlled sample app is launched, exercised, and captured; artifact receipts bind source/build/process/capture identities. Build-only and screenshot-only runs cannot satisfy behavioral playtest criteria. |
| AC09 | Restore previews and applies only covered paths; an intervening edit causes a conflict with no overwrite. Crash recovery never reports a mixed restore as successful. External side effects remain explicitly non-restorable. |
| AC10 | Forks preserve source lineage but have independent conversation state, review freshness, and write ownership. Starting the branch requires valid current authority and a visible budget allocation. |
| AC11 | Search locates an older persisted message outside the bounded activity window; rename/pin/archive survive restart. Opening an archived or paused conversation does not start inference. |
| AC12 | Receipt, incorporation, supersession, and withdrawal races have deterministic outcomes; no acknowledged withdrawal is included in a later request. Activity is tied to observed events rather than timer guesses. |
| AC13 | A permission grant cannot exceed host policy; narrowing fences later actions. Approval receipts remain scoped and signed where required, and stale approvals cannot authorize changed actions. |
| AC14 | Retry/restart/fork cannot reset a governing budget; usage omissions display as unknown, not zero cost. Reaching a limit prevents new admissions and produces a recoverable handoff. |
| AC15 | Saved guidance is inspectable with provenance/scope; forgetting excludes it from future retrieval and invalidates affected summaries without claiming deletion from immutable audits or past provider requests. |
| AC16 | Diagnostics have bounded time/output, make no repair mutations, redact credentials, and report unsupported checks honestly. A failed login probe never automatically reinstalls a provider. |
| AC17 | `/init` produces an exact proposed file diff and command inventory; rejection leaves the workspace unchanged; approval preserves existing instruction files and requires separate trust for command execution. |
| AC18 | Old fixtures decode unchanged; unsupported feature mutations reject; state migration and rollback tests preserve pending work. Required native-platform visual and interaction evidence is recorded before release. |

## Current architecture

The source is authoritative over older design prose. In particular, model/effort selections now
apply to subsequent turns; documentation describing an idle-only model switch predates that behavior.
Read-only planning is distinct from authorized in-place work, which already routes through the
production pipeline. Several older prose descriptions also lag the current always-visible tool summaries.

| Existing owner and inspected source | Existing foundation and design implication |
| --- | --- |
| [TUI command reducer](../crates/app/peritus-tui/src/model/chat/commands.rs), [chat model](../crates/app/peritus-tui/src/model/chat.rs), [renderer](../crates/app/peritus-tui/src/render/chat.rs) | Local slash dispatch, mutable draft, panels, and composer-adjacent timer; extend these with typed command metadata and focused panel reducers. |
| [TUI runtime](../crates/app/peritus-tui/src/runtime.rs), [candidate execution](../crates/app/peritus-tui/src/runtime/candidate.rs) | Own terminal restoration and foreground launch. Current candidate launch checks its digest and directly spawns the parsed command; do not mistake it for daemon-owned preview/capture lifecycle. |
| [A3 product contracts](../crates/app/peritus-app-protocol/src/product.rs), [feature negotiation](../crates/app/peritus-app-protocol/src/version/feature.rs), [registry](../crates/app/peritus-app-protocol/src/schema/registry.rs) | Bounded typed IPC, stable tag allocations, generated assets and compatibility fixtures; extend additively and negotiate new feature groups. |
| [Daemon interaction](../crates/app/peritus-daemon/src/product_run/interaction.rs), [lifecycle](../crates/app/peritus-daemon/src/product_run/lifecycle.rs), [persistence](../crates/app/peritus-daemon/src/product_run/persistence.rs) | Run-bound public activity, received/incorporated revisions, cancellation/retry/recovery, and atomically replaced durable JSON records. There is no durable user pause state in the current run phase enum. |
| [Developer interaction port](../crates/orchestration/peritus-agent/src/developer/interaction.rs), [provider-turn execution](../crates/orchestration/peritus-agent/src/developer/execution/provider_turn.rs) | Immutable per-turn provider selection, durable input acknowledgement, and safe public observations; extend boundary checks, not a second tool loop. |
| [Product runner](../crates/app/peritus-product-runner/src/execution.rs), [candidate recorder](../crates/app/peritus-product-runner/src/execution/checkpoint.rs) | Existing writer/check/reviewer/fixer delivery and evidence freshness must remain authoritative. Candidate qualification is distinct from goal completion and rewind coverage. |
| [Accounting](../crates/app/peritus-product-runner/src/budget.rs), [public progress](../crates/app/peritus-daemon/src/product_run/progress.rs) | Aggregate request/tool/retry/token/cost/resource counters and hard ceilings exist; expose and extend them into durable cross-attempt user limits. |
| [Local context](../crates/app/peritus-product-runner/src/local_context/mod.rs), [configuration](../crates/app/peritus-product-runner/src/context_config.rs), [storage](../crates/app/peritus-product-runner/src/local_context/storage.rs) | `inspect_local_context`, `LocalContextHandle`, local journal/artifact storage, and deterministic assembly already exist. Local auxiliary compaction is optional and explicitly configured. |
| [C6 compaction](../crates/orchestration/peritus-context/src/compaction.rs), [working state](../crates/orchestration/peritus-context/src/working/state.rs), [memory record](../crates/orchestration/peritus-memory/src/record.rs) | Typed provenance, validated replacements, working state, and reusable memory lifecycles exist; expose controlled projections without elevating model-authored memory into policy. |
| [Workspace images](../crates/app/peritus-product-runner/src/workspace_media.rs) | Bounded image discovery and provider capability checks exist; first-class import receipts and artifact-linked feedback are missing from the inspected composer path. |
| [Workspace authorization](../crates/runtime/peritus-workspace/src/authorization.rs), [gateway](../crates/runtime/peritus-workspace/src/gateway.rs), [Git snapshots](../crates/runtime/peritus-git/src/snapshot.rs) | Reuse checked authority and snapshot operations. Plain-folder checkpoints need covered-path before-images; whole-tree Git restoration is not safe for arbitrary concurrent user edits. |
| [Launcher daemon](../crates/app/peritus-launcher/src/daemon.rs), [provider setup](../crates/app/peritus-launcher/src/provider_setup.rs), [workspace setup](../crates/app/peritus-launcher/src/workspace_setup.rs) | Existing readiness, provider selection, and trust flows supply diagnostic observations and approved repair entrypoints. |

Rust edition 2024, pinned Rust 1.97.1, ratatui 0.30.2, crossterm 0.29.0, tokio, rusqlite,
and the existing C0 stores are the baseline. No dependency, toolchain, or workspace-wide lint change
is required by this design. Any later platform integration dependency requires a separate review.
Retain existing layer boundaries and formal/ordinary Rust verification policy.

Related repository designs: [conversation-first interface](conversation-first-interface.md),
[plain-folder workspaces](plain-folder-workspaces.md), [local working memory](local-working-memory.md),
[single-command product](single-command-product-experience.md), and
[production architecture](peritus-production-architecture.md). CL-36 and CL-54 cover related existing
product/handoff work; CL-65 tracks this design rather than claiming those issues are completed.

## Proposed design

### Ownership and control flow

Keep presentation in G2, durable user operations and admission in G0/G4, execution boundaries in
D0/G4, context/memory semantics in C6, authority in B1/C1, and storage in C0. Introduce small modules
under the existing owners rather than extending already-large files or adding an omnibus controller.
No orchestration crate may depend upward on the application protocol. A3 DTOs map to domain types
at the daemon; the runner owns execution contracts without importing TUI state.

```text
TUI/CLI typed intent + expected revision + operation identity
    -> daemon authorization and feature checks
    -> authoritative journal transaction / operation receipt
    -> existing runner's next safe admission boundary
    -> existing provider/tool/gate/reviewer execution
    -> observed result + evidence references + durable state
    -> revisioned public projection -> TUI panels and command receipts
```

Inspection requests do not invoke a model or mutate the inspected state. Mutations are acknowledged
only after durable acceptance, not merely socket receipt. The same domain operation serves TUI and
scriptable CLI; neither embeds a second implementation of policy or goal completion.

### Goals: persistent objectives rather than repeated prompting

`/goal objective` opens a brief with proposed criteria, constraints, execution mode, role models,
and limits. Model-proposed criteria are suggestions until accepted. If an unambiguous previously
approved brief exists, reuse it and show the binding. Empty `/goal` only inspects. Replacing an active
goal is an explicit superseding operation, not an unnoticed interpretation of another chat message.

Maintain one active goal per conversation initially. A goal references one or more existing run
attempts in the same workspace lineage; it is not a `RunId`, a provider turn, or a Git candidate.
Different conversations may have goals, but workspace mutation ownership and host budgets still apply.

States are `active`, `waiting-for-user`, `pausing`, `paused`, `blocked`, `budget-reached`, `achieved`,
and `cancelled`. Persist the last transition reason, outstanding criteria, evidence bindings,
pending effects, budget counters, and restart eligibility. User-facing state does not collapse these
into the current `ProductRunPhase::terminal()` boolean.

At a settled execution boundary, the host evaluates typed criteria: exact command outcome and target,
required source/artifact existence, applicable gate/review evidence, and explicitly requested human
validation. A selected reviewer may assess qualitative criteria through the existing provider route;
that invocation is visible and charged to the same budget. No hidden evaluator model or provider
switch is introduced. Model statements are proposals for a verdict, not independent evidence.

An achieved goal requires every mandatory criterion to be satisfied against its latest requirement
revision and relevant artifact/workspace digest. No unresolved effects, blocking findings, or missing
playtest evidence may be silently omitted. Explicit human acceptance of a subjective criterion is
recorded as human evidence, not a passing automated test or a waiver of unrelated technical failures.

If incomplete, admit another segment only when there is actionable work, sufficient budget, valid
authority, and no unresolved external dependency. Reuse the current progress-based continuation
and bounded recovery logic. Identical failures or inspection-only loops cannot extend work forever;
surface a blocked handoff after the bounded recovery policy is exhausted. A waiting process or
external dependency is not evidence of progress or failure by itself; wait on its owned event source,
with bounded backoff and no repeated inference polling.

`/goal clear` cancels future goal continuation and requests a safe pause of goal-owned execution;
it preserves the goal/history and any completed effects. `/stop` still requests cancellation.
`/quit` only detaches. Reopening shows an active daemon-owned goal but does not duplicate it.
After a crash, recovery revalidates authority, workspace, pending receipts, and remaining budget;
user-paused, cancelled, blocked, or budget-reached states never auto-resume.

### Pause, resume, live activity, and queued steering

Add an execution-control port at existing provider and effect admission boundaries. Persist the pause
request before signalling the worker. `after-operation` permits the already-admitted operation to
settle, then publishes a resumable checkpoint and `paused`. During a provider call, it may complete
but no returned tool action starts. `before-edit` may continue permitted read-only work but stops
before any mutation-capable action, including shell commands not proven read-only. `now` requests
provider/process cancellation through existing ownership; it displays cancellation pending until
termination or an explicit ambiguous-outcome record is observed. Do not promise immediate suspension.

Resume obtains current ownership, validates effects and filesystem preconditions, reconciles
partial/ambiguous operations, rebinds the next model request, and resumes the same logical goal.
It does not restart a completed command, silently reset a budget, or discard review findings.
On a changed workspace, mark stale context/evidence and require inspection before further mutation.

Replace a single opaque pending follow-up experience with an ordered durable input ledger. Each item
has stable identity, content revision, dependencies, author provenance, queue order, and state:
`queued`, `held`, `incorporated`, `superseded`, or `withdrawn`. A request captures a concrete immutable
list of item IDs/revisions under the same transaction that marks incorporation. No reordering of
history that was already incorporated is allowed.

Editing a queued item creates a superseding revision. Editing incorporated text creates a new
correction; it cannot pretend an in-flight request never saw the old text. Reordering across declared
dependencies rejects. Withdrawal and incorporation race under a compare-and-swap revision: exactly
one wins, and the loser receives an actionable stale-state response. Immediately fence stale pending
tool calls when a correction is accepted, preserving the existing input-revision safety behavior.

The compact activity display uses typed host observations, not parsed prose. Show operation label,
last observed result, queue receipt/incorporation, and pending approval/pause. File counts derive from
the tracked change set, not model claims. Provider silence remains `waiting for provider`; elapsed time
is not a claim of model progress. An optional completion/attention terminal bell is opt-in and contains
no private message text; desktop notifications are not required for this program.

### Task brief, attachments, context, and compaction

The brief presents the objective, acceptance criteria, constraints, referenced files/images, current
execution mode, and unresolved assumptions. Each field retains its source message or user edit.
Separate `user-confirmed`, `agent-proposed`, and `observed` content visually and in the data model.
Accepting an assumption does not grant additional tool permissions. Conflicting requirements are
shown for resolution rather than silently overwritten by a generated summary.

A brief edit increments the governing requirement revision, invalidates affected completion/review
evidence, and fences pending stale effects. Preserve unaffected evidence only when the existing
evidence rules prove the binding still valid. Do not restart the entire conversation unnecessarily.

File references resolve within the selected workspace with identity, digest, and selected range.
Offer explicit snapshot versus refresh-on-next-request semantics, and show which applies. At admission,
a refreshed file is read through existing authorized inspection and produces a new digest receipt.
Never resolve references to another workspace by basename alone. A textual mention can remain text
when the user declines attachment; do not imply bytes were sent.

Image/file import uses existing artifact transfer and provider media support. Imported bytes are
immutable artifacts with digest, media type, dimensions where applicable, byte size, source label,
and visibility scope. Validate magic bytes, decode bounds, dimensions, count, and aggregate limits;
do not merely trust file extensions. The current workspace-image path caps individual/aggregate
media; new UI limits must not exceed provider/host ceilings and must report exclusions explicitly.
Missing image capability is an error before submission, not silent text-only substitution.

The user must initiate clipboard import or choose a path. Do not scan the clipboard in the background.
Treat absolute paths outside the workspace as explicit imports requiring a preview and confirmation;
reading them does not expand future filesystem authority. Show what will be sent to the selected
provider. For large text, offer explicit range selection rather than silent truncation.

`/context` has `next request` and `invocation history` views. Show source category, role, digest,
selection reason, pin state, byte count, token estimate/report provenance, and whether the item was
included. Distinguish eligible context from the actual sealed request manifest. Inspection uses
the published local-context checkpoint and never reconstructs by running recovery or inference.
Do not expose credentials, unavailable provider internals, or private chain-of-thought.

Users may pin requirements, decisions, and source references or exclude optional retrieval items.
They cannot remove mandatory host instructions, authorization constraints, unresolved effects, or
tool-result pairs needed for a valid request. If mandatory content exceeds capacity, block the next
request with a concrete explanation rather than silently dropping constraints.

`/compact [focus]` creates a proposed prompt-view generation using C6 validated compaction and the
existing local context host. Keep the exact archive intact. Preserve accepted brief fields, budget
and permission bindings, recent complete exchanges, unresolved findings, failed approaches with
sources, pending operations, and handles for older evidence. The optional focus is a user preference,
not permission to discard unrelated safety state.

Default compaction is deterministic/local; the existing optional local subprocess stays opt-in.
No model download, remote memory service, or remote compaction fallback is added. If local semantic
support is unavailable, explain whether deterministic compaction can meet the bound. Publish the
new view atomically only after validation; old views remain inspectable and available for recovery.

### Interactive changes and artifact feedback

Extend `/diff` into a structured change-review panel without removing raw diff inspection. A file/hunk
selection can receive `explain`, `request revision`, `keep behavior`, or `leave alone` feedback.
An explanation is read-only and uses exact source/diff references; a revision request is explicit
work subject to ordinary admission. Comments have states such as open, addressed, stale, or dismissed
by the user; the model cannot declare them resolved without current supporting evidence.

Anchor comments with workspace lineage, before/after blob digests, path, hunk context digest, and
selected range. Line numbers alone are not identities. If later changes prevent exact rebinding,
mark the comment stale and ask for a new anchor; never apply an old instruction to a coincidentally
matching line. Show related check/review receipts and their freshness. Partial acceptance of hunks
must invalidate whole-candidate qualification unless the resulting candidate is rechecked.

`Leave alone` creates a revisioned user constraint over an exact path/range. File tools enforce it
through the existing mutation gateway. Shell or build operations that might write those targets need
actual sandbox restrictions; if a backend cannot enforce the restriction, disallow that operation
or ask the user to explicitly revise the constraint. Merely adding text to a prompt or checking after
a destructive write is not enforcement. Explain the difference between a hard protected path and
a non-enforceable semantic preference such as `keep this behavior`.

Apply the same comment model to screenshots, documents, and other artifacts. Anchor feedback to the
artifact digest, optional image coordinates, and source/build identity. New artifacts never silently
inherit a resolved verdict from an older screenshot. Text/image contents remain untrusted evidence.

### Launch, preview, screenshots, and evidence

The result panel groups changed files, launchable outputs, checks, review findings, and validation
artifacts. Show `built`, `launched`, `captured`, `behavior checked`, and `human reviewed` independently.
The user can launch an unfinished result, but it remains explicitly unqualified.

Introduce a typed launch profile: executable plus argument vector, working directory, required
environment references, workspace/artifact digest, readiness observation, resource/time limits,
network posture, and stop policy. Discovery may propose a profile from manifests or an approved
`/init` configuration; commands are not executed during discovery. Avoid interpreting arbitrary
shell fragments embedded in prose as launch instructions.

Move new managed preview lifecycles behind daemon-owned process admission, with an explicit handoff
to terminal ownership when necessary. Reuse existing process handles, cancellation, resource bounds,
and streamed output. Do not silently replace the existing `/run` implementation before parity tests.
Extend preview to plain folders through tracked source identities and explicit launch authorization;
do not claim managed Git candidate semantics for those folders.

The launch operation rechecks source/artifact freshness, policy, and budget. A GUI build may spawn
a child process: capture identity must follow the owned process tree or a user-selected window,
not a title guessed from the desktop. Stop owned preview processes explicitly; detaching the TUI
must not create orphaned unbounded servers. Show whether a preview remains running after detach.

Native capture uses a capability-checked backend. On Wayland/macOS or other consent-requiring hosts,
request the supported user-approved capture flow; denial is not a reason to capture the whole desktop
through another route. Browser capture targets the selected page. Keep access scoped to the chosen
window/page, never adjacent apps. A text-only terminal offers artifact metadata and an explicit
local-viewer action; inline image rendering is optional and capability-detected.

Store a capture receipt containing source/build digest, launch operation/process identity, capture
time and target, image digest, dimensions, capture backend, and preceding observed interactions.
Automated interaction adapters are bounded and target only the approved app/page. If unsupported,
offer a user-performed checklist with explicit human attribution, not fabricated automation evidence.
Opening a local artifact viewer must not execute embedded scripts or arbitrary URL handlers silently.

For the motivating Tetris case, a playtest records launch, readiness, movement/rotation/drop, pause,
restart, and observed resulting states. A screenshot proves appearance at an instant; it does not
alone prove controls, collision rules, or absence of crashes. Attach automated logs and human notes
as distinct evidence classes. Feed user-selected screenshots back through the ordinary media path
when the user requests visual corrections.

### Checkpoints, rewind, and alternate approaches

A user checkpoint is a manifest over a conversation revision, context-view references, brief and
goal revision references, workspace lineage, covered path versions, and outstanding effect receipts.
It is not a copy of credentials, granted permissions, active process handles, or reusable approvals.
Record its exact coverage: agent-touched files, explicitly included files, excluded generated/large
files, and external effects that cannot be restored. No hidden whole-folder backup is implied.

Capture automatically before the first admitted mutation of a logical work segment once checkpointing
is enabled, and support explicit `/checkpoint name`. Snapshot covered bytes before modification;
collect before/after digests for creates, replacements, removals, and renames. Reuse Git snapshot
primitives for registered worktrees, but do not rewrite the user's index or branch as a side effect.
Plain folders use content-addressed before-images plus a covered-path manifest, without requiring Git.

Checkpoint capture is quiescent with respect to owned mutations. If files change during capture,
retry within a bound or return unavailable; never publish a manifest assembled from inconsistent
generations. Limits and exclusions are visible before it is presented as a recoverable checkpoint.

Rewind first pauses owned execution and prepares a restore plan. For each covered path, compare
current bytes/identity to the known post-change version. Restore only if preconditions hold; leave
unrelated paths untouched. A file created by Peritus can be removed only if it still matches the
recorded created version. Preserve later user edits and show a conflict rather than guessing ownership.

Before applying, retain the current covered versions as a recovery checkpoint. Journal a restore
operation with staged replacement artifacts, per-path preconditions, publication receipts, and a
final commit marker. Do not promise a filesystem-wide atomic transaction. A crash can leave an
incomplete restore; recovery reports it and reconciles only exact known versions, never overwriting
new third-party changes while attempting a rollback.

External editors are not necessarily governed by Peritus leases. An implementation must prove its
compare-and-replace/exclusive-access guarantees on the target backend. Where that cannot be enforced,
offer a restored isolated copy or an exportable patch rather than claiming safe unattended in-place
rewind. A user-facing confirmation alone does not make a racy implementation safe.

Conversation-only rewind starts a new logical history branch at the selected revision; retain the
original audit and make the divergence from current files explicit. Files-only rewind keeps the
conversation but inserts a restore observation and invalidates stale context/evidence. Combined
rewind publishes the new conversation branch only after the file operation settles. None of these
restore old permissions, reset cumulative budgets, or undo messages, deployments, database writes,
package installations, or other external effects.

`/fork` preserves parent/checkpoint references and creates independent conversation/goal state.
The forked goal is a draft until explicitly started. Writable alternatives use isolated registered
worktrees or approved covered-file copies; do not run two writers against the same plain folder.
A conversation-only fork can remain read-only against the current workspace, clearly labeled as
not a historical filesystem snapshot. Carry source-backed context, not live tool IDs, pending actions,
or qualification for a different workspace. Forking never automatically publishes or merges branches.

### Conversation library and continuation

Add user-facing conversation identity separate from transport `SessionId` and execution `RunId`.
The library groups conversations by workspace and supports title, pin, archive, last activity,
goal state, and a plain-language next-action summary. Old run-bound conversations receive stable
legacy mappings; transport reconnect does not create duplicate library entries.

Search durable public messages, user-approved briefs, and safe artifact labels using a local,
rebuildable index. Do not search only the bounded activity projection. Private reasoning, credentials,
and raw sensitive provider traces are not indexed. Literal full-text search is sufficient initially;
no embeddings or background inference are required. Snippets retain source message references.

Opening a result shows what changed, which checks passed/failed, why execution stopped, and what
remains. Distinguish running daemon work from resumable but stopped work. Library navigation,
renaming, pinning, and archiving never start inference. Archive is reversible and separate from
deletion. Deletion is not part of this program; existing exact-candidate discard controls remain.

### Usage and budgets

Expose aggregate and per-role/request consumption with explicit provenance: provider-reported,
locally derived, estimated, or unavailable. Preserve original usage observations and reconcile
cumulative provider updates without double counting. Display currency only when the source defines
its unit and conversion; current `provider_cost_microunits` alone is not a verified invoice currency.

Support user limits on active execution time, admitted requests, tool calls, and tokens, plus an
estimated-cost stop threshold only where the route supplies usable cost data. Time excludes deliberate
paused intervals but includes admitted provider waits and retries; show wall elapsed separately.
Account work across designer, writer, reviewer, fixer, and any explicitly requested evaluation.

Effective limits are the minimum of user limits and applicable host ceilings. Reserve capacity before
admitting an operation and reconcile after its result. Persist reservations, debits, and unresolved
charges; crash/retry never credits uncertain usage back as though the operation were free. Existing
per-attempt ceilings remain safety backstops, not substitutes for the cumulative goal ledger.

A provider may report tokens or cost only after completion and cancellation may not stop billing.
Therefore label a cost threshold as a best-effort stop threshold unless a route supplies enforceable
pre-admission bounds. Reject requests for a guaranteed monetary cap on unsupported routes and offer
time/request limits instead. Do not show missing usage as zero. A long already-admitted operation may
consume its reserved allowance after a threshold; disclose that boundary.

When a limit is reached, stop new admissions, settle or request cancellation of owned work according
to its contract, and publish a `budget-reached` handoff with retained changes and outstanding checks.
Resume requires a deliberate limit change or an available remaining allocation. Increasing limits
does not widen permissions or auto-resume. Forks receive explicit child allocations under the same
governing ceiling by default; a newly authorized independent budget is visibly separate, not a reset.

### Permissions, project memory, and initialization

`/permissions` shows effective workspace trust, read/write scope, process/network capabilities,
approval requirements, and provenance of each restriction. Separate one-operation approval,
conversation grants, and project policy. A user can narrow permissions immediately for subsequent
admissions; broadening requires the existing explicit authority/approval path and cannot exceed host
policy. Changes increment an authority revision, stale incompatible queued actions/approvals, and
leave in-flight outcomes visible. Signed approvals stay bound to exact operations and revisions.

`/memory` manages user-approved reusable project guidance separately from agent-authored working
hypotheses. Show text, source, scope, lifecycle, and last validation. Saving is explicit; a model may
propose a record but cannot silently mark it user-approved. Scope is project-first; cross-project
reuse requires a deliberate scope change. Private guidance lives in local state by default.
Exporting guidance into repository instruction files is a separate reviewed file mutation.

Forgetting writes a tombstone, removes the record from future retrieval, rebuilds affected indexes,
and invalidates summaries/views that depend on it. It does not erase past prompts already sent to
a provider or claim secure deletion from immutable journals, backups, or exports. Show that distinction
before confirmation. A request to purge historical storage would need a separate retention/privacy
design; it is not silently implemented by `/memory forget`.

`/init` inspects selected manifests, existing instructions, local documentation, and command
configuration. Produce proposed build/test/lint/launch profiles and an exact instruction-file diff.
Prefer a bounded Peritus-managed section or a separate approved file over replacing an existing
`AGENTS.md`. Mark commands discovered from files as unverified until explicitly run with appropriate
trust. Do not execute package scripts, install dependencies, or contact providers during discovery.
Apply only the reviewed diff with filesystem preconditions; rejection changes nothing.

### Diagnostics

`/doctor` checks installed binary identity/version agreement, daemon endpoint and negotiated
features, durable-store readability, selected provider executable/credential-reference readiness,
workspace identity/trust, configured tool availability, and required preview/capture capabilities.
Use bounded safe probes; do not print credential values or arbitrary provider environment variables.

Classify findings as healthy, warning, blocked, or unsupported with a concrete suggested action.
Opening diagnostics does not repair the database, regenerate config, reinstall providers, start
inference, restart active work, or upload logs. Local provider-auth probes may use existing supported
read-only status commands; any network probe must be disclosed and separately requested if needed.
Reuse launcher repair flows only after a separate action preview and explicit confirmation.
Diagnostic export is a local redacted artifact chosen by the user, with a preview of included data.

### Alternatives considered

| Alternative | Tradeoff and decision |
| --- | --- |
| Prompt-only slash macros | Low initial cost, but cannot reliably implement pause, receipt races, budgets, or restore. Use prompts only to propose brief/qualitative content; reject as the control architecture. |
| A new agent runtime or separate UX daemon | Offers isolation but duplicates provider/effect ownership and recovery. Extend existing owners and ports instead. |
| A GUI-first rewrite | Easier rich previews, but replaces the current interaction surface and expands platform scope. Keep protocol-backed TUI panels and explicit external artifact viewing; a later GUI can reuse them. |
| Whole-folder snapshots and unconditional restore | Simple concept but expensive and dangerous for user edits, generated artifacts, and plain folders. Use covered manifests, preconditions, and isolated restore fallback. |
| A new database per feature | Independent implementation is tempting but complicates cross-feature atomicity. Use one authoritative product-control journal with artifact references and rebuildable projections, reusing C0 patterns. |
| Immediate global migration of all legacy state | Simplifies eventual code but risks existing runs and downgrade safety. Upgrade selected conversations through an explicit generation boundary and keep legacy records available read-only. |

The preferred design costs more than prompt aliases, but its admission, receipt, revision, and
artifact contracts can be tested independently of model behavior and shared by future clients.

## Data and compatibility

### Proposed domain records

These names describe responsibilities, not preallocated wire tags or already-existing Rust APIs.
Use bounded domain types, private fields with validated constructors, typed errors, and canonical
encoding under the existing owner. Keep secrets and raw media out of public DTO debug output.

| Record | Required bindings and fields |
| --- | --- |
| ConversationRecord | Stable conversation/workspace lineage; title, archive/pin state; branch parent; public message references; active goal and last handoff references. |
| TaskBriefRevision | Objective, criterion IDs, constraints, references, assumptions and user-confirmation provenance; monotonically increasing requirement revision. |
| GoalRecord | Goal identity, brief revision, execution mode, run-attempt references, state/reason, continuation eligibility, evidence verdicts, governing budget identity. |
| ControlOperation | Idempotency identity, actor/session binding, expected aggregate revision, typed intent, accepted revision, execution receipt, terminal result. |
| ExecutionControl | Desired pause/cancel state, control revision, safe-boundary observation, outstanding effect identities, resumable checkpoint reference. |
| InputQueueItem | Message identity and content revision, queue order/dependencies, author, receipt time, state, incorporating invocation or superseding revision. |
| ContextManifest | Conversation/brief/role/invocation binding, view generation, source digests/ranges/provenance, inclusion reasons, token-count provenance, mandatory pins. |
| AttachmentRecord | Artifact digest, validated type/size/dimensions, import origin label, approved scope, inclusion receipt and provider/media capability result. |
| ReviewComment | Author, intent, exact file/hunk or artifact anchor, state, supersession, addressing evidence, any separately admitted hard constraint. |
| UserCheckpoint | Covered-file manifest and before/after digests, conversation/context references, capture generation, external-effect exclusions, retention references. |
| RestoreOperation | Checkpoint identity, expected filesystem versions, staged artifacts, per-path receipts, recovery checkpoint, final publication marker/conflicts. |
| LaunchProfile / ValidationReceipt | Executable/argv/cwd and policy; source/build/process identities; readiness/interactions/capture artifact references and evidence class. |
| BudgetLedger | Typed limits, units, parent/child allocations, reservations, debits, uncertainty, active elapsed accounting and threshold state across attempts. |
| ProjectGuidance | User-approved content, provenance, scope, lifecycle/tombstone and dependent context references, mapped to existing C6 memory semantics. |

Separate conversation history, control state, context views, and file versions. They may reference
each other but do not share one mutable blob whose rewrite silently changes past evidence.
Store long text, exact prompts, images, and checkpoint bodies as artifacts; public projections carry
bounded previews and digests. Add pagination and stable cursors before exposing whole-session search
or artifact lists. Enforce byte, count, depth, image pixel, queue, checkpoint, and index limits with
typed overflow errors and visible retention choices; no unbounded user-controlled allocation.

### Authoritative publication and recovery

Use a product-control journal through existing C0 facilities for upgraded conversations. The daemon
owns ordered control events, expected-revision transactions, and idempotent receipts. Current run
JSON records and local-memory journals remain owned by their respective components; do not claim
that writes across those stores are one atomic transaction.

Publish artifacts before the journal record that references them; verify digests and retain a
garbage-collection grace period for unreferenced staged artifacts. A control intent commits first;
the runner observes its revision before admitting another effect and publishes its observed result
afterwards. Projections can lag but must show their revision. Recovery replays the authoritative
intent and effect receipts rather than trusting a stale UI or run JSON summary.

For upgraded conversations, the control journal is the sole authority for goal/pause/queue/budget
state. Run JSON is not a second writer of that state. Failure to access a required control revision
stops effect admission. A successful legacy run snapshot cannot override a pending pause or revive
a budget-reached goal. Idempotency keys survive reconnect and are bound to the request payload digest;
reusing a key with different content rejects.

External effects and multi-file restore are recoverable operations, not exactly-once promises across
arbitrary systems. A receipt that proves completion can be replayed. An interrupted command without
a conclusive result remains ambiguous and requires reconciliation or explicit user direction.

### Wire and stored-state evolution

Add independently negotiated feature groups for goal/control, context management, input queue,
workbench artifacts/review, checkpoint/fork, conversation library, budgets, and configuration controls.
Allocate new tags only through the current A3/B3 registries; this document reserves no numeric tags.
Regenerate the checked-in schema, TypeScript declarations, registry, and compatibility fixtures.
Never change a legacy tag's payload shape or reinterpret an existing phase as `paused`.

An unsupported server advertises the missing capability and rejects mutations; the client retains
drafts and does not fall back to sending the command as prose. An older client must not be allowed
to resume or mutate an upgraded paused/budget-constrained conversation through a legacy control path.
Enforce required-feature gating at operation admission, not only at command visibility.

Use an explicit product-state generation/minimum-reader marker for upgraded control state. Retain
legacy records as immutable migration inputs, with stable ID mappings and source checksums. Import
on explicit upgrade or first opted-in feature use, transactionally publishing the new conversation
root only after validation. Legacy conversations remain usable without silent goal conversion.

Because old binaries do not understand a new pause state, a marker alone is insufficient protection.
The upgrade must activate a distinct state/config generation that old launchers cannot interpret as
eligible legacy work. Do not leave an executable legacy recovery copy pointing at the same writable
workspace. Before migration commits, fence legacy recovery for the moved runs and verify no older
worker owns them. Rollback to an older binary is an explicit stopped/read-only export flow, never
`ignore unknown fields and continue`. Migration qualification must exercise this downgrade hazard.

This is additive product metadata, not a rewrite of existing applied C0 migration digests. Any new
database schema uses append-only migrations with backup/integrity checks. Importing context, artifact,
and budget references must not duplicate charges or elevate trust. Indexes are projections and can
be rebuilt from authoritative records without model calls.

### Retention

Show checkpoint/artifact storage consumption and configured limits. Default garbage collection may
remove only unreachable staged data or explicitly expired unpinned artifacts under the retention
policy. Active goals, pending restore operations, pinned checkpoints, and required evidence retain
their dependencies. If storage is full, fail before claiming a new checkpoint is safe. Do not evict
the only undo data for active changes to make a progress operation succeed.

## Failure handling

| Failure or race | Required behavior |
| --- | --- |
| Client disconnects after submitting a command | Reconnect queries its operation identity; return the original receipt or safely retry admission, never duplicate execution. |
| Store full/corrupt or artifact publication interrupted | Retain prior committed generation; reject new dependent effects and explain recovery requirements. |
| Pause/correction races with tool admission | One ordered admission boundary decides; report already-admitted work honestly and fence subsequent stale effects. |
| Queue withdrawal races with incorporation | Exactly one revision transaction wins; an incorporated item can only receive a later correction, not retroactive withdrawal. |
| Provider unavailable or authentication fails | Retain goal/context; enter a concrete blocked state after allowed recovery. No hidden provider/model/effort change. |
| Model repeatedly returns unsupported completion claims | Keep mandatory criteria unresolved; bounded recovery/continuation ends in a truthful blocked handoff, not success. |
| User edits a file after a checkpoint or review | Mark affected anchors/evidence stale; restore conflicts do not overwrite new bytes. |
| Capture permission denied or window identity ambiguous | Report capture unavailable and preserve existing artifacts; no whole-desktop fallback. |
| Preview crashes or never becomes ready | Retain exit/output evidence, apply bounded timeout/cleanup, and mark launch/playtest criterion unsatisfied. |
| Budget expires with an operation in flight | Stop new admissions, follow cancellation/settlement policy, retain uncertain usage and incomplete-check handoff. |
| Permission narrows while work is active | Fence new incompatible operations; report in-flight effects and stale approvals explicitly. |
| Compaction proposal loses required sources | Reject it and retain the previous published view; never silently reduce authority or acceptance requirements. |
| Search/index unavailable | Fall back to bounded durable listings and offer a separately scoped index rebuild; no loss of conversations. |
| Old client/binary cannot understand upgraded state | Inspect through a supported read-only projection or reject with upgrade/export guidance; never guess execution eligibility. |

Every blocked state names the failed condition, effects already completed, evidence retained, and
the smallest user action needed. No failure path silently claims completion, resets an accounting
baseline, or discards the source needed to diagnose the failure.

## Security considerations

- The user's explicit scope governs execution. A goal, restored checkpoint, imported image, memory
  entry, or repository instruction file cannot broaden authority by itself.
- Preserve read-only planning/review across every new surface, including memory writes to the
  workspace, launch profiles, initialization, context import, and explanation actions.
- Treat tool output, search hits, diagnostics, model suggestions, and file/image contents as
  untrusted data. Slash-looking content is never dispatched as a command.
- Bind operations to actor, workspace, conversation, expected revision, and exact action digest.
  Reconnect/fork/rewind cannot clone or revive signed approvals, leases, or permission grants.
- Sanitize terminal text and links, bound previews and decoders, validate paths and symlink/hardlink
  behavior, and keep private artifacts out of public notifications and diagnostic exports.
- Screenshot capture and launch have independent permission checks. Launching generated code can
  execute arbitrary project behavior; a passing build or model recommendation is not authorization.
- Do not index hidden reasoning or credentials. Context inspection shows authorized model-visible
  inputs and source-backed summaries, not provider-private internals.
- Keep local memory local except for content explicitly selected into an ordinary provider request.
  Do not add background uploads, remote embeddings, telemetry, or cloud synchronization.
- Hard leave-alone constraints must be enforced below the model and across all capable tools.
  If a command/backend cannot meet them, refuse or obtain an explicit constraint revision.
- No user control can disable host ceilings or bypass existing acceptance, review, and effect gates.

## Verification

### Automated layers

Use deterministic scripted providers for replayable feature tests; add live provider checks only
as explicitly authorized compatibility evidence. Do not attribute a scripted response to a live model.

1. Pure reducers: goal transitions, criterion verdicts, queue ordering/dependencies, expected revisions,
   budget reservations, context pinning, stale anchors, checkpoint coverage, and guidance tombstones.
2. A3/C0 compatibility: old fixtures, new tagged operations, missing-feature behavior, canonical
   roundtrips, idempotency receipts, migration imports, projection rebuilds, and downgrade fencing.
3. Runner/daemon composition: actual production provider/effect boundary, real temporary workspaces,
   existing schema/authority/review gates, pause/resume, retry accounting, and receipt-based recovery.
4. Fault injection: crash before/after artifact write, intent commit, effect admission, result receipt,
   restore-path publication, and projection update; disk-full, truncation, stale versions, and reconnect.
5. Security: readonly command rejection, path escapes, malicious terminal sequences/images, stale
   approvals, conflicting leases, process-based protected-path writes, and redacted diagnostic exports.
6. TUI reducer/rendering: every command, argument completion, focus/selection, draft preservation,
   overlays, narrow layouts, long histories, Unicode, missing capabilities, and truthfully labeled waits.

Run formatting, strict affected-package all-target/all-feature Clippy, affected tests, full-workspace
compile checks for public API changes, generated protocol checks, documentation checks, and applicable
repository/formal-boundary gates. Do not suppress warnings or relax existing thresholds to land UI work.
Record unrelated baseline failures separately; a slice is not globally qualified while required gates fail.

### Composed user acceptance scenarios

| Scenario | Required evidence and coverage |
| --- | --- |
| V1: Tetris from brief to playtest | Attach a reference image; confirm brief/goal; build through the real pipeline; launch and exercise controls; record screenshot and behavioral receipts; comment on the captured result. Covers AC02, AC06-08, AC14. |
| V2: Correct and pause a long operation | Submit two follow-ups, reorder/withdraw one, pause during provider/tool work, restart daemon/client, inspect state, and resume without duplicate effects. Covers AC01-03, AC12-14, AC18. |
| V3: Explore and safely reject an approach | Capture checkpoint; edit via Peritus; add an independent user edit; attempt rewind and inspect conflict; fork into isolation; prove original bytes/history remain. Covers AC07, AC09-10, AC18. |
| V4: Find and reconstruct earlier understanding | Search beyond the activity window; open a paused conversation; inspect exact context; compact it; verify pinned requirements and evidence; forget guidance and inspect future retrieval. Covers AC04-05, AC11, AC15. |
| V5: Setup without hidden side effects | Run `/doctor` with broken auth/tool paths and `/init` in a project with existing instructions; decline repairs/changes, then approve one exact diff. Covers AC13, AC16-18. |
| V6: Bounded work and stale evidence | Reach a small request/time limit, restart, attempt legacy resume and branch budget reset, revise a criterion/file, and prove prior completion evidence cannot satisfy the new revision. Covers AC02-03, AC07, AC10, AC14, AC18. |

### Native visual and interaction gate

Run the actual rebuilt product in a real terminal emulator and inspect captured screenshots. A
ratatui `TestBackend` snapshot is useful but insufficient. Exercise wide and narrow windows, resizing,
scrolling, focused panels, pasted multiline input, image import, queued corrections, provider waits,
paused/completed states, and reconnect. Confirm the working row remains immediately above the composer.

Qualify Linux X11 and Wayland capture paths separately, plus macOS and Windows terminal/clipboard/
process/capture behavior where those capabilities are offered. Preserve platform, terminal version,
binary/source digest, dimensions, actions, and resulting screenshots. Unsupported capture backends
must show a tested unavailable path; they do not count as successful graphical validation.
Use isolated state/workspaces so validation never disturbs the user's active work or captures other apps.

Performance acceptance includes a bounded searchable archive fixture, paginated responses, bounded
memory, and input responsiveness during background index/capture work. Set fixture sizes and measured
latency budgets in P0, record the hardware, and reject regressions against that baseline; do not invent
production performance claims from empty-session unit tests.

## Rollout and rollback

### Delivery order

Each phase delivers end-to-end behavior with protocol, persistence, host enforcement, UI, docs, and
tests together. A disabled placeholder command is not a completed phase. Optional panels/features
must accurately reflect negotiated availability. Shared infrastructure must not become an open-ended
refactor of unrelated crates.

| Phase | Code owners and complete deliverable | Data/compatibility impact | Required completion evidence |
| --- | --- | --- | --- |
| P0: command/control foundation and diagnostics | A3 feature negotiation and bounded command DTOs; G0 control receipt/journal adapter; G2 command metadata/panels; launcher-backed `/doctor`. Establish explicit public activity fields. | New negotiated feature group and versioned product-state generation scaffolding; legacy fixtures unchanged. | AC01, AC16, relevant AC18; V5 diagnose-only path; native keyboard/resize screenshots and measured UI baseline. |
| P1: brief, inputs, and context | G2 attachment/brief/context/queue panels; G0 immutable input ledger; D0 incorporation binding; G4 media/local-context adapters; C6 pins and validated `/compact`. | Message/attachment/context manifests and queue receipts; atomic incorporation migration for upgraded conversations. | AC04-06, AC12; V2 queue races and V4 context/compaction; provider image-capability rejection. |
| P2: bounded persistent goals | G0 goal/control state; G4 accounting ledger and coordinator; D0 safe-boundary port; G2 `/goal`, `/pause`, `/resume`, `/usage`, `/budget`. | Durable goal/control/budget records and recovery eligibility; old-client resume fence; no reset across attempts. | AC02-03, AC14; V2 and V6 crash/budget matrix. Goal criteria needing later preview support remain visibly unsatisfied/unavailable. |
| P3: conversational change review | G2 structured diff/comment UI; G0 anchors and revisions; G4 evidence mapping; C1/C4 protected-path enforcement. | Anchored feedback and hard-constraint revisions; invalidate affected qualification when code/requirements change. | AC07, AC13 enforcement subset; stale-hunk and process-write bypass tests, actual diff-panel interaction screenshots. |
| P4: launch and inspect the result | G4 launch/validation records; daemon-owned process integration; G2 result/artifact viewer and feedback; reviewed native capture adapters. | Launch profiles, capture receipts, artifact references and explicit backend capabilities. | AC08 and remaining visual AC06; V1 including actual app interactions and source-bound screenshots, cancellation/consent-denial tests. |
| P5: safe checkpoints and rewind | C1 covered snapshot/restore operations; G0 restore journal and conflict recovery; G2 preview/confirm UX. | Checkpoint manifests, before-images, restore-operation receipts and retention roots; explicit unsupported in-place restore fallback. | AC09; V3 user-edit and crash-at-every-publication tests on each offered backend. No release of unattended restore without enforceable preconditions. |
| P6: conversation library and branching | G0 conversation identity, legacy mapping and local search index; G2 `/sessions` and `/fork`; C1 isolated workspace binding. | Parent/child references, rebuildable full-text index, budget allocation and permission revalidation for forks. | AC10-11; V3/V4/V6, migration/idempotent library mapping, no inference on navigation. |
| P7: explicit project controls and cohesive qualification | G2 `/permissions`, `/memory`, `/init`; G0/launcher approval and reviewed-diff flows; C6 guidance lifecycle. Polish shared command discovery and handoffs. | Scoped policy/guidance revisions, tombstones, reviewed project instruction/profile updates. | AC13, AC15, AC17 and full AC18; V1-V6 composed end-to-end; all advertised native platforms qualified. |

P0 precedes mutation features. P1 precedes sustained goals so accepted instructions and context have
one reliable representation. P2 supplies pause/accounting for later preview/restore work. P3 precedes
artifact feedback; P4 precedes claiming graphical goal completion. P5 precedes writable historical
forks in P6. P7 completes the public configuration controls, while their underlying enforcement remains
mandatory in every earlier phase. Do not expose a command early if its host enforcement is deferred.

After review, create bounded implementation issues per phase with explicit dependencies and design
subsections. Do not dispatch the whole program as a single coding prompt. No implementation is
authorized by CL-65 or this document alone.

### Feature enablement and rollback

Enable each capability only when its complete host/client contract and required tests are present.
Keep existing conversation-only behavior available for legacy state. Run an opt-in isolated-state
canary before enabling upgraded control state for existing user conversations.

Rollback of UI code can hide entrypoints, but it cannot remove durable enforcement. A paused or
budget-constrained goal must remain protected even if the UI feature is disabled. Drain/pause owned
operations before state-generation migration or binary downgrade; preserve checkpoints, artifacts,
ledger debits, pending receipts, and audit lineage. Never downgrade by deleting fields or modifying
applied migration digests. Export an explicitly read-only legacy-compatible view when necessary.

If a platform-specific preview backend fails qualification, advertise it as unavailable on that
platform while retaining artifact inspection/import where supported. Do not substitute generated
screenshots, fabricated interaction receipts, or another platform's result as proof.

## Open questions

The following defaults make this a complete reviewable proposal without inventing user approval.
They can be revised before implementation; any change to a public contract updates the design and
its acceptance matrix first.

| Decision for product review | Proposed default |
| --- | --- |
| Goal creation confirmation | Always show criteria/scope/limits before starting a newly parsed goal; no-confirm fast path is outside the first release. |
| Qualitative completion | Reuse the explicitly selected reviewer with visible accounting; require human evidence for specifically human-requested validation. |
| Default panels | TUI-first, keyboard-complete; responsive side panels where space permits and full-width panels on narrow terminals. |
| Checkpoint coverage and storage | Covered paths only; visible quotas and pins; never a silent full-folder backup or eviction of active undo data. Exact quotas are configured and measured during P0/P5. |
| Fork behavior | Conversation fork is non-running by default; writable historical alternatives require isolated files/worktree and explicit budget/authority admission. |
| Monetary limits | Best-effort estimated-cost thresholds only on routes with usable reporting; reject unsupported guaranteed monetary caps. |
| Memory persistence | Local project-scoped user-approved guidance; repository export and wider scope require separate confirmation. |
| Capture availability | Native capability/consent checks per platform; explicit unavailable or manual-evidence path when automation is unsupported. |

Architecture verdict: ready for design review and phase scoping, not blanket implementation kickoff.
Residual high-risk work is enforceable restore under external writers, durable migration/old-binary
fencing, cross-store effect recovery, and native capture/process ownership. Those risks have explicit
acceptance gates above; they must not be converted into weaker guarantees during implementation.

## Out of scope

- Implementing, committing, pushing, installing, or launching this program during the design task.
- A new desktop/web application, cloud synchronization, remote memory/embedding services, or telemetry.
- A second agent runtime, automatic model substitutions, hidden evaluation providers, or new swarm orchestration.
- General scheduled jobs, multi-project autonomous backlog execution, or automatic commits/publishing.
- Whole-system rollback, guaranteed reversal of external effects, or secure erasure of historical provider data.
- Broad dependency/toolchain upgrades, unrelated repository refactors, or weakened formal/security/test gates.
- Implementing every command in other products; this design covers the improvements explicitly discussed.

## References and provenance

Repository links in Current architecture were inspected at the baseline above. Existing design prose
is historical context; it is not proof that an interface is implemented. Crosslink knowledge searches
for `context` and `product` returned no matching pages during this design pass.

External primary references refreshed on 2026-09-09 UTC:

- [Claude Code commands](https://code.claude.com/docs/en/commands): precedent for discoverable context, goal, rewind, and diagnostic commands; no promise of command parity.
- [Claude Code goals](https://code.claude.com/docs/en/goal): precedent for completion-driven continuation. Peritus's proposed goal ledger and evidence admission are its own design, not a copy of that evaluator policy.
- [Claude Code checkpointing](https://code.claude.com/docs/en/checkpointing): precedent for explicit conversation/file restore choices; not evidence that arbitrary external effects are reversible.
- [GitHub agent sessions](https://docs.github.com/en/copilot/how-tos/copilot-on-github/use-copilot-agents/manage-and-track-agents): precedent for session history and progress inspection; this proposal remains local-first.

These sources are product references, not instructions or authorization. This document makes no claim
that a competitor feature establishes Peritus's correctness, security, or measured performance.

## Design validation and future handoff

The design-only validation pass checked all 35 local document/source links, required design sections,
R01-R18 to AC01-AC18 coverage, all 16 proposed command names, fenced-block balance, and whitespace.
`cargo xtask docs-check` passed for its 151 maintained documents; the separate checks above explicitly
cover this new `.design` file. No implementation tests or native-platform qualification for these
proposed features have been run. Their required evidence is specified, not claimed complete.

The companion pipeline sidecar records the document digest and the `designed` stage with no plans
or runs. That stage means a design artifact exists, not that its implementation is approved.

After product review and explicit authorization of P0, an appropriately bounded kickoff would be:

```sh
crosslink kickoff run "Implement only approved phase P0 of the interactive workbench design; all other phases remain out of scope" --doc .design/interactive-workbench-and-harness-controls.md --verify local
```

This is a future handoff example, not a command executed during this design task. Scope later phases
as separate approved implementation issues with the dependencies and acceptance gates above.
