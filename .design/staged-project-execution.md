# Feature: Staged project execution and automatic issue tracking

Status: proposed implementation design. The product behavior below records the user's decisions from the design discussion. Writing this document does not start implementation.

## Summary

Give Peritus a built-in, durable issue tracker and a decomposition agent that turns a design document into small tasks organized by project maturity. Stage workers execute those issues through the agent loop. The daemon owns task selection, durable results, automatic issue closure, and advancement to the next stage.

The governing principle is **constrain the work an agent may perform**. There are no task, stage, project, token, turn, retry-count, monetary, or elapsed-time execution budgets introduced by this feature. Tasks should ordinarily take two to five minutes of focused work, but that is a decomposition target, never a cutoff. Productive work continues. Oversized assignments are split while retaining their progress.

The five stages are Skeleton, Coverage, Integration, Hardening, and Optimization. Every stage uses the agent loop with instructions and completion conditions specific to that stage. Each stage starts with its own context lineage. Individual task attempts and independent reviewers also have focused contexts. Continuity comes from the issue tracker, current code, real-operation evidence, and structured handoffs rather than inherited conversations.

## User-visible behavior

### From design to work

1. The user selects a saved Design draft or a Markdown document and asks Peritus to break it into work. Peritus captures an immutable version of that document and its source identity.
2. The decomposition agent reads the design and relevant repository evidence. It creates a requirement map, the full subsystem/stage outline, dependency relationships, and executable issues for near-term work.
3. Peritus presents the stage plan, existing functionality it intends to preserve, material unresolved requirements, and the first eligible tasks. Planning does not itself authorize edits. An existing explicit instruction to implement the design can authorize execution without another approval round; otherwise Start project starts it.
4. The stage loop works eligible issues. The interface shows the current stage, issue, expected outcome, why it was selected, observed progress, and any dependencies or deferred findings.
5. After a task satisfies its scoped acceptance and review, Peritus durably records its result and automatically closes the issue. The user and worker do not need to remember a close command.
6. Once all required stage outcomes are satisfied, Peritus records a handoff and continues automatically in the next stage with a fresh context. The user can inspect, steer, pause, or stop the project throughout.

Proposed TUI entrypoints are `/project` for the selected project, `/project plan` for decomposition, `/project start`, `/project pause`, `/project resume`, and `/issues` for the tracker. `/issues show ID` opens an exact issue. These names are proposals for the shared client contract, not commands implemented today. The GUI exposes the same project, stage, issue, evidence, and control operations through discoverable controls. Neither client implements a separate scheduler.

Example status:

```text
Project: Native module support
Current stage: Coverage
Subsystem outcomes: 9 of 12 minimally functional
Issue P-42: Persist module enablement across restart
Why next: Required by module activation
Current action: Read back the saved selection after restarting the disposable instance
Deferred P-57: Profile startup time during Optimization
```

Issue detail explains its parent, originating requirements, stage, dependencies, acceptance conditions, claim/attempt, results, follow-ups, and closure reason. Closed, cancelled, and superseded items have distinct labels. Counts distinguish executable tasks, container issues, completed work, deferred work, and unresolved requirements.

## Requirements

| ID | Requirement |
| --- | --- |
| R01 | Provide a local issue tracker owned by the daemon and shared by TUI, GUI, CLI, workers, and reconnecting clients. External accounts, GitHub, Crosslink, containers, and paid APIs are not prerequisites. |
| R02 | Bind a project to an immutable design revision and preserve traceability from every agreed requirement to its stage outcomes and issues. Record exclusions and unresolved decisions explicitly; agents cannot silently delete requirements. |
| R03 | Run a dedicated decomposition role through the agent loop. It creates and revises the complete stage/subsystem outline and small executable issues with dependencies and observable acceptance. Planning tools cannot edit program code or start workers. |
| R04 | Apply the five-stage maturity model across the whole agreed project scope. A worker cannot independently advance maturity or select ineligible later-stage work. Later-stage dependency exceptions require recorded evidence and the smallest necessary remedy. |
| R05 | Aim for tasks completed in a few minutes, ordinarily two to five minutes of focused work. Introduce no execution budgets or quota-based stopping. Record discovered work, split oversized issues, and continue from retained progress. |
| R06 | Give every admitted task an explicit, versioned contract identifying permitted work, relevant requirements, stage, dependencies, acceptance, and live authority. Writer, reviewer, and fixer operate within that same contract. |
| R07 | Give each stage its own context lineage, separate from previous stages. Task attempts and role contexts remain distinct. Transfer structured outcomes and resolvable evidence; do not prepend prior-stage transcripts or share writer scratch memory with the independent reviewer. |
| R08 | Use the real-testing skill for behavioral acceptance appropriate to the task and stage. Build or startup success alone cannot prove an implemented feature works. Do not introduce unit-test, formal-verification, or repository-qualification requirements as automatic completion gates. |
| R09 | Automatically close a successful issue as part of host-owned task settlement. Closure requires a current task result, appropriate observed evidence, and scoped review; model prose or calling a close tool is insufficient. |
| R10 | Calculate parent and stage completion from required work and their explicit acceptance outcomes. Child counts alone cannot bypass required integration evidence. A satisfied earlier-stage outcome does not satisfy a later-stage requirement by implication. |
| R11 | Recover claims, attempts, results, closures, and stage handoffs after disconnects, provider failures, daemon restarts, and indeterminate writes. Replaying an operation must recover its original receipt without duplicating closure or blindly replaying external effects. |
| R12 | Incorporate steering and design changes through revisioned operations. Reject stale task settlements and invalidate only affected future eligibility or evidence. Preserve historical outcomes and explicit cancellation/supersession reasons. |
| R13 | Enforce claim ownership and workspace mutation ownership across clients and workers. Begin with one mutating task per shared workspace while allowing other projects and read-only inspection within available resources. |
| R14 | Continue eligible work when one issue is blocked. Show actual dependency, capability, or decision blockers when no work is eligible; never claim completion because the queue is empty or a provider failed. |
| R15 | Keep project state and context scalable through per-record state, immutable artifact references, pagination, and incremental projections. Do not serialize an entire project history, all source text, or all issue bodies into one bounded run record. |
| R16 | Support greenfield and existing repositories. Inventory and demonstrate existing behavior before crediting completed outcomes; preserve functioning code and user data without replaying unnecessary earlier work. |
| R17 | Make project and issue operations inspectable and recoverable through the actual TUI and GUI, with matching scriptable queries and commands. Preserve ordinary single-run and module workflows. |
| R18 | Expose real progress and whether the planner's task sizing improves delivery. Timing and usage are observations, not termination thresholds. |

## Acceptance criteria

The implementation is complete only after these observable journeys pass on the delivered program. Section Verification describes how to exercise them without replacing the actual application with a simulated tracker or provider.

| ID | Action and required observation | Requirements |
| --- | --- | --- |
| A01 | Decompose a saved design and a selected Markdown file. Each original requirement is represented, explicitly excluded by the user, or shown unresolved; every planned issue has an origin and stage. Reopen the plan after daemon restart. | R01–R03, R11 |
| A02 | Plan a project containing several required subsystems, including an unimplemented one. Complete an attractive subsystem and propose optimization. The project remains at its earliest incomplete stage and the optimization issue is deferred. | R04, R10 |
| A03 | Demonstrate a bottleneck that prevents an earlier-stage acceptance journey. Admit a narrowly scoped dependency exception with its evidence and target issue; reject a worker's unsupported assertion that optional polish is a dependency. | R04, R06 |
| A04 | Run a Skeleton task through implementation, scoped review, and repair. Required later-stage functionality remains visible as open work without preventing valid Skeleton closure. Coverage cannot credit the same stub as functioning behavior. | R04, R06, R08, R10 |
| A05 | Execute several small issues and a task that grows after discovery. Observe automatic closure for completed work, a retained partial result and child issues for the larger assignment, and no time/token/attempt-budget termination. | R05, R09 |
| A06 | Follow a complete task through real operation, independent scoped review, settlement, automatic closure, and selection of its successor. No worker or user close command is issued. Issue detail identifies the exact result and evidence. | R06, R08, R09 |
| A07 | Return a confident completion message while required behavior fails, omit evidence, or reference evidence from a stale attempt. The task remains open with an actionable finding and enters repair or recovery. | R08, R09, R12 |
| A08 | Finish a stage, then inspect the next stage's exact provider request manifests and context identities. Required handoff facts are available, previous-stage transcript markers are absent, and writer scratch markers are absent from reviewer context. | R07 |
| A09 | Interrupt before result publication, after evidence publication, after settlement commit, and before the client receives the receipt. Reconnect/restart and resume. Each successful issue has one effective closure and no next task starts ahead of its settlement. | R09, R11 |
| A10 | Stop or lose a worker with an owned command still running. Recovery reconciles the actual process/effects before another owner can mutate the same workspace; silence or a missed heartbeat never proves success or abandonment. | R11, R13 |
| A11 | Submit competing claims and stale close/replan operations from two real clients. One exact claim wins; conflicting requests receive an inspectable rejection and a fresh query remains usable. | R11–R13 |
| A12 | Change a relevant design requirement during a task and submit its old result. The old outcome remains historical, stale closure is rejected, and affected issues are revised/reopened. An unrelated design edit does not erase valid completed work. | R02, R12 |
| A13 | Close all children of a parent that still requires a cross-subsystem journey. The parent/stage stays incomplete until that journey passes. Cancelled or superseded children do not count as successful implementations. | R09, R10, R12 |
| A14 | Block a task while an independent task is ready. The independent issue proceeds. Introduce a dependency cycle or an unresolved product choice; show its cause and affected work without spinning or silently narrowing scope. | R03, R14 |
| A15 | Start from a complex repository copy with working features, missing features, and retained data. Demonstrate credited behavior, implement gaps, exercise restart/recovery, and preserve unrelated files and data. | R08, R16 |
| A16 | Create a project large enough to require multiple issue/evidence pages, then extend it. Navigate and resume without loading the entire project into a prompt or one run JSON record, and without a fixed issue-count execution cutoff. | R07, R15 |
| A17 | Use the TUI and GUI to plan, inspect issues, start, steer, pause, reconnect, resume, and inspect closure evidence. Both reflect the same authoritative project state. A headless client can query the same issue and receipt. | R01, R17 |
| A18 | Complete tasks with ordinary Chat/Design and native modules still available. An older client cannot accidentally start or edit staged work; unsupported protocol generations preserve readable historical artifacts. | R11, R17 |
| A19 | Demonstrate all five stage loops on an actual small application, including a measured Optimization issue. Observe automatic handoffs and show outcome/evidence differences at each stage. | R04, R06–R10 |
| A20 | Record expected versus observed task duration, repeated inspections, and substantive outputs during a sustained campaign. Long work can continue while making progress; stalled work is surfaced and replanned from evidence without a budget-exceeded result. | R05, R14, R18 |

## Current architecture

Source grounding was performed on `develop` at `87dedeaa80b3e286970ac38e12a22ebc35446266`. This section describes inspected owners, not an exhaustive repository audit or a claim that this feature already exists.

| Existing owner | Relevant behavior and consequence |
| --- | --- |
| [Guided design artifact](../crates/app/peritus-product-runner/src/guided_design.rs) and [Design conversation policy](../crates/app/peritus-product-runner/src/execution/conversation/design.rs) | Save a versioned, run/workspace-bound Markdown draft. Its `Stage` enum describes interview progress, not project maturity. Capture this draft as a project input; do not reuse its stage tags. |
| [Conversation handoff](../crates/app/peritus-product-runner/src/execution/conversation/tools.rs) | `run_pipeline` requests the existing implementation pipeline using the governing conversation. Add a deliberate project-plan/start route rather than changing every conversational handoff implicitly. |
| [ProductRunner composition](../crates/app/peritus-product-runner/src/execution.rs) and [run contracts](../crates/app/peritus-product-runner/src/execution/types.rs) | Compose design, writing, checks, review, fixing, and settlement. `ProductRunPhase` is an execution phase inside a run; it is separate from the five project maturity stages. |
| [Public obligation capture](../crates/app/peritus-product-runner/src/execution/obligations.rs) | Captures conversation clauses as acceptance-critical obligations. Passing the whole project conversation into every tiny task would ask each task to satisfy the entire project. A typed task obligation scope is required. |
| [Acceptance](../crates/app/peritus-product-runner/src/execution/acceptance.rs) and [settlement](../crates/app/peritus-product-runner/src/execution/settlement.rs) | Own delivery-specific evidence and terminal settlement. Add explicit task outcomes and acceptance policy; do not infer issue closure from a generic successful run, zero exit code, or summary string. |
| [Pure workbench controls](../crates/app/peritus-product-runner/src/control.rs), [goal DTOs](../crates/app/peritus-app-protocol/src/workbench/goal.rs), and [daemon goal control](../crates/app/peritus-daemon/src/product_run/workbench/goal.rs) | Supply revisioned goals, input control, ownership, safe boundaries, and evidence freshness. They do not currently represent the proposed project/stage/subsystem/issue hierarchy. |
| [Product control journal](../crates/app/peritus-daemon/src/product_control/storage.rs) | Commits events, successor state records, and exact operation receipts together through C0. Provides the persistence pattern for automatic closure and indeterminate-write reconciliation. |
| [Context port](../crates/app/peritus-product-runner/src/local_context/port.rs) | Coding roles have existing lineages; `open_module` already derives a separate job/stage identity. Extend the scoped-context pattern for projects; do not reuse module namespaces or silently fall back to the global writer lineage. |
| [Engineering guidance](../crates/app/peritus-product-runner/src/engineering_workflow/AGENTS.md) | Owns writer/reviewer guidance, real testing, computer use, and conditional Rust reliability. Stage/task contracts must reach every role here and the actual execution/acceptance paths. |
| [TUI queue](../crates/app/peritus-tui/src/model/chat/workbench/queue.rs), [web gateway](../crates/app/peritus-web/src/api.rs), and [browser console](../webui/src/lib/components/Console.svelte) | Existing client/control surfaces to extend. A durable user-input queue is distinct from the project issue tracker; adding an issue must not fabricate a user instruction or grant authority. |
| [D3 scheduler](../crates/orchestration/peritus-scheduler/README.md) | Owns a separate run-scoped durable scheduling aggregate. Its availability does not establish integration with the human product execution path. Reuse appropriate concepts while keeping one authoritative project claim/settlement owner. |

The production route remains client → daemon `ProductRunService` → `ProductRunner` → `DeveloperLoop` → provider and host tools. The journal-backed domain agent/orchestrator drivers remain separate machinery. Implementation must not create a second executor that bypasses the human product permissions, command lifecycle, or recovery paths.

## Proposed design

### 1. Core ownership and alternatives

Implement project records, issue transitions, maturity policy, and settlement invariants as a core Rust domain component, proposed name `peritus-projects` under `crates/orchestration/`. This is a proposed new package, not an existing source path. Keep provider calls and filesystem/process effects outside its pure transitions.

The daemon owns the authoritative store, planner invocation, claims, workspace reservations, task admission, result reconciliation, and client projections. The product runner executes a task through the existing role loop using an explicit task contract. Clients and models submit typed proposals and inspect receipts; they do not write authoritative issue state.

Prefer this over a prompt-only planner or plugin-only tracker: those alternatives leave closure, admission, and restart behavior dependent on a particular conversation or plugin being alive. An external tracker adapter may later mirror local records, but network availability must never determine local task completion. A new independent task executor is also rejected because it would duplicate the authority and recovery machinery already needed by coding runs.

### 2. Project hierarchy and requirement coverage

Use the display hierarchy **project → maturity stage → subsystem outcome → issue/task**. Requirements link across the hierarchy, and dependency edges form an acyclic graph across issues. A tree alone cannot express cross-subsystem prerequisites.

The project has one agreed scope revision containing design provenance, requirements, exclusions, and required outcomes. It can contain optional backlog work, which is displayed separately and does not inflate required completion counts. Workers cannot demote required work to optional or edit exclusions.

Every requirement has an explicit status: unresolved, planned, partially delivered, satisfied at a stated maturity stage, or superseded by an authorized revision. Satisfaction cites accepted outcomes; the absence of an issue is never satisfaction. Stage-specific criteria clarify what the requirement means at that stage. Placeholder registration during Skeleton cannot satisfy the Coverage behavior of the same requirement.

Container issues have no independent worker claim. Executable leaves have one concrete outcome. A container may require its own acceptance journey, represented as an explicit dependent executable issue so it cannot disappear behind child counts.

For existing repositories, the planner creates assessment issues for relevant existing behavior. A successful assessment can credit a stage outcome without changing code, but it must reference actual inspected artifacts and appropriate real operations. No automatic credit based only on file existence, README claims, or an old agent's summary.

### 3. Maturity policy

| Stage | Permitted focus | Completion condition | Evidence appropriate to the stage |
| --- | --- | --- | --- |
| 1. Skeleton | Responsibilities, modules, interfaces, stubs, minimal composition | Every required subsystem has an explicit home and declared boundary; the skeleton is usable for subsequent work | Build and launch the skeleton, inspect exposed registrations/interfaces, and demonstrate explicit unimplemented behavior where applicable. This establishes structure, not feature functionality. |
| 2. Coverage | Minimum required behavior for every subsystem | Every required subsystem performs its minimum advertised function | Operate each changed subsystem through its actual interface or a minimal real public consumer and inspect the result. |
| 3. Integration | Wiring and contract repairs across subsystems | Required end-to-end user journeys operate using the actual components | Exercise complete CLI/TUI/GUI/service journeys and read back resulting state. |
| 4. Hardening | Error handling, recovery, security, preservation, robustness | Required adverse-condition and recovery outcomes are demonstrated | Restart, interrupt, reconnect, supply invalid inputs, exercise permission failures and relevant resource conditions, and inspect retained state. |
| 5. Optimization | Demonstrated bottlenecks and justified simplification | The agreed measured improvement is achieved while required behavior remains intact | Retain a baseline, measure the changed behavior under comparable conditions, and replay directly affected real journeys. |

The current project stage is the earliest required stage with an unsatisfied outcome under the agreed scope revision. Track each subsystem's progress too; do not average away missing required subsystems. Show complete, active, pending, and reopened stage states independently of execution phases.

A task is normally eligible only when its stage is the current stage, its prerequisites are satisfied, its contract is current, and live permissions/resources allow it. Reopened earlier-stage defects take precedence over forward advancement. Earlier-stage maintenance required to preserve functioning behavior is admitted as an explicit repair issue. Basic correctness, required security protections, and protection of user data apply throughout; Hardening adds depth rather than authorizing unsafe earlier work.

A dependency exception names the blocked required issue, observed failure, necessary later-stage remedy, restricted scope, expected unblocking behavior, and governing scope revision. The coordinator may admit it automatically when those conditions are demonstrated. It remains an exception with an explanation, not a declaration that the project advanced. Speculative elegance or presumed future performance is insufficient. A model cannot issue itself an exception by changing a stage label.

The scope is the user's agreed project/design scope. Agents cannot manufacture a small replacement project to evade the overall maturity rule. New requirements are revisioned explicitly; unrelated optional backlog does not perpetually prevent completion of an agreed delivery.

### 4. Decomposition agent

The planner receives the immutable design, relevant current user steering, repository map, existing accepted outcomes, and existing issue/dependency state. It uses read-only repository tools plus narrowly scoped planning-proposal tools. It has no program edit/process tools and cannot directly assign work, close implementation issues, grant permissions, or change agreed scope.

It performs the following logical work through the agent loop:

1. Extract exact requirements and unresolved choices, retaining source references.
2. Map the complete required subsystem/stage outline and preservation constraints.
3. Assess known existing outcomes and create evidence-backed assessment issues where needed.
4. Detail dependency-ready and near-term issues into small executable assignments.
5. Submit a versioned plan delta for host admission. Record later-stage detail progressively as prerequisites reveal facts.

The complete outline is durable from the start, while task detail develops progressively. An unexploded future outcome remains visibly outstanding and cannot count as a finished stage. There is no fixed planning horizon count, issue quota, or requirement to invent thousands of speculative tasks before useful implementation can begin.

Plan admission checks source/revision identity, requirement coverage, stage assignment, graph integrity, existing outcome preservation, and whether executable issues have concrete acceptance. The agent loop reviews semantic decomposition quality; host validation handles structural invariants. Structurally valid planning is not proof that the proposed implementation will meet the requirements.

Workers propose newly discovered issues, splits, and prerequisite repairs. The coordinator deduplicates proposals using source requirement, affected behavior, and existing issue references, then durably admits the relevant delta. A task result must not silently discard discovered mandatory work.

### 5. What counts as a small task

A task describes one observable outcome and an executable assignment, ordinarily two to five minutes of focused work. It identifies entrypoints and relevant dependencies so the worker does not have to rediscover the project. Small does not mean one arbitrary file or a token-limited patch. A tiny change can legitimately touch several callers to preserve a contract.

Separate tasks such as "display installed module names and enablement", "persist an enablement change", and "read the saved selection after restart" instead of "implement module management". Assign cross-subsystem acceptance separately once its prerequisites exist. Reuse valid builds and setup where their provenance remains current rather than repeating expensive setup per issue.

If investigation reveals several outcomes, record a retained partial result and replace the executable leaf with a container plus smaller child issues in a single admitted plan delta. A worker can continue productive work at an ordinary safe boundary while that delta is settled. Completed child work can receive its own evidenced settlement; the original broad issue does not close prematurely. Preserve discovery results and code progress so splitting does not restart the task from scratch.

Timing records help improve future sizing. Exceeding an estimate does not stop a task. Repeated reads, repeated unsuccessful operations, or an unchanged blocker should trigger diagnosis and replanning based on actual observations, not an arbitrary retry counter. Necessary long builds or coherent operations may continue, with their state visible.

### 6. Scoped execution through the existing agent loop

Introduce a typed task execution binding containing the project, scope revision, stage session, issue/contract revision, attempt, claim epoch, and governing user revision. The daemon supplies it; text in a prompt cannot create it.

Each stage uses the production design → writer → real-operation checking → independent review → fixer → settlement composition, operating on its admitted tasks. An already accepted stage design and exact task contract can supply the task-local design; invoke the designer again when a material technical question remains unresolved. Do not regenerate a design document or repeat project discovery for every small issue. Review and fixing may continue until the assignment is satisfied or a real blocker/pause requires another disposition. There is no fixed iteration allowance.

The contract includes the task objective, requirement references, dependency outcomes, permitted work, relevant source locations, preservation rules, acceptance scenarios, applicable stage policy, and known deferred issues. Tools retain live host authority and any explicit path restrictions. If the worker needs a material scope expansion, it proposes a plan/contract update rather than treating its own inference as permission.

The project requirement ledger stays intact. Task acceptance selects the applicable requirement outcomes for this stage and task; other requirements stay pending at project scope. This is an explicit typed acceptance mode, not deletion of obligations or a prompt asking the reviewer to ignore them. Audit obligation capture, all role prompts, findings, fixer routing, candidate settlement, and user-facing completion wording together.

In particular, the current full-conversation obligation capture cannot remain the effective acceptance scope for every staged task. Project text is reference material; a scoped task outcome must not claim project-wide qualification. Existing host checks for authority, checkpoint validity, and durable effects remain in force. Staged acceptance uses real-operation evidence and must not inherit unrelated deterministic test/qualification suites merely because the legacy broad delivery path advertised them.

The reviewer receives the task contract, exact relevant changes, independent requirements, real-operation evidence, and project preservation constraints. It returns in-scope blockers, demonstrated prerequisite regressions, and deferred findings separately. A later-stage preference becomes an issue; an actual introduced data-loss or authority defect remains a blocker in every stage. Findings need observable consequences or a concrete contract violation, not speculative redesign demands.

The host can enforce which task is claimed, permitted operations, identity/revision fences, and declared path constraints. Semantic scope adherence also requires inspecting the actual diff, observed behavior, and scoped review. A stage tag alone cannot prove an agent stayed on task.

### 7. Context and handoff ownership

Give the planner, each stage coordinator, each task attempt, and each independent role an explicit durable context identity. A proposed identity tuple is `(project, scope revision, stage generation, issue, attempt, role)`, with a distinct planner/coordinator variant. A stage retry recovers its own lineage; advancing stages creates a new lineage. Re-entering an earlier stage after a requirement change creates a new stage generation.

Do not map these identities onto the existing generic writer lineage or the module context namespace. Writer/fixer continuity may share the task's validated working state where appropriate; the reviewer remains independent. A new task receives relevant outcomes, not the previous task's private scratch work.

The host assembles each context from:

- The authoritative stage/task contract and current relevant user instructions.
- Applicable engineering, real-testing, computer-use, and conditional language guidance.
- A compact complete project/stage index with resolvable requirement references.
- Relevant dependency outcomes, exact source references, accepted evidence, and known blockers.
- Explicitly selected reference pages needed for the current work.

Source documents and handoff prose are evidence, not new system authority. Do not flatten the entire original conversation into every task's governing instructions. Preserve user intent through exact revisioned references and relevant steering. Retrieve large source/evidence bodies on demand through existing paged artifact/context mechanisms. Provider transport capacity is handled by those mechanisms; it is not a project execution budget or a reason to forget completed work.

At a stage transition, publish an immutable handoff with the design/scope version, accepted issue/outcome references, code/workspace checkpoint, actual behavior established, open/deferred issues, prerequisite exceptions, artifact references, and next-stage entry conditions. Keep previous-stage transcripts available for deliberate inspection, but absent from the new stage's default prompt.

The next stage obtains a fresh identity and reads the accepted handoff and current code. If handoff publication or context creation fails, retain the previous completed stage and a pending transition. Recovery resumes transition setup without reopening completed work or marking the next stage started prematurely.

### 8. Task lifecycle and automatic closure

An executable issue moves through `planned → ready → claimed → working → checking → closed`. Readiness is derived from current dependencies, stage policy, and blockers. `blocked` is explicit with its cause. Pause preserves the active claim/attempt at a safe boundary. `cancelled` and `superseded` are distinct terminal dispositions, not successful closure. Reopening creates a new issue revision/attempt while retaining the prior result.

A claim is a durable ownership record, not an elapsed-time lease. Its epoch fences old workers after recovery or reassignment. A missed heartbeat, client disconnect, or provider timeout cannot alone transfer workspace authority. Reassignment requires explicit reconciliation of the previous task, owned processes, and effects.

The worker returns a `TaskResultDraft`: exact task binding, outcome proposed, change/checkpoint references, acceptance observations, scoped review references, unresolved findings, and follow-up proposals. There is no model-accessible "mark done regardless" operation. A user can cancel or revise work, but that disposition cannot manufacture successful implementation evidence.

The host settlement protocol is:

1. Validate the current claim, issue/contract/scope revisions, governing steering, live permissions, and candidate/workspace binding.
2. Publish the actual evidence and result artifacts durably, obtaining immutable references. Task success requires its declared observable outcome; completion prose, a build, or source containing the expected answer is insufficient.
3. Validate applicable evidence, resolved in-scope review, and retained mandatory follow-ups. If discovery expands what this issue itself requires, replan or repair instead of using a follow-up to conceal incomplete acceptance.
4. Commit one authoritative settlement transaction containing the result reference, issue successor/disposition, ownership release, admitted follow-up references, and original operation receipt. Any derived count updates included in the transaction use the same revision fence.
5. Publish the committed result to clients. Only now may this worker claim its next issue. Parent/stage eligibility is derived from authoritative issue settlements, never an optimistic UI count.

An unsuccessful check routes back to repair or a recorded blocker, retaining the issue as unfinished. A terminal model reply without a usable result causes host reconciliation of the observed work. It does not silently close the issue, and it does not require a user to remember a close command.

Use the existing C0 append/state-install/receipt transaction pattern for steps that require atomic visibility. Large artifacts are stored before the transaction; the transaction references them. If a plan delta is too large for one transport/journal frame, stage its immutable records and atomically publish its root before it can affect eligibility. Do not announce success for a partially published plan.

The coordinator can update large ancestor projections incrementally after settlement. Scheduling and stage advancement must consult committed issue/outcome state, so a stale projection cannot allow premature progress. Stage completion, handoff reference, and transition intent are durably committed with exact operation identities before launching the next stage. Parent acceptance is represented explicitly, avoiding recursive parent closure that accidentally skips a required workflow.

### 9. Steering, reopening, and no eligible work

Users may revise the design, reprioritize eligible work, clarify an issue, pause, or stop. New user instructions outrank earlier design proposals. Authority expansions still use the existing permission system. Ordinary implementation refinements within the agreed requirements can be planned automatically; changes to product scope or unresolved material product choices remain explicit user decisions.

Analyze a revision's affected requirements, contracts, dependencies, and evidence. Preserve valid unrelated outcomes. A historical result remains true of its recorded artifact even when a later code change requires a new acceptance run. Reopen affected issues or create linked regression work; do not erase history or reopen the entire tracker merely because the workspace digest changed.

An unaffected active attempt may continue through an explicit host-recorded rebinding to the new scope revision after its contract, relevant requirements, dependencies, and authority are confirmed unchanged. Preserve its context and valid observations. An old result cannot simply assert that it is unaffected or settle against an obsolete binding.

If a task blocks, record the missing dependency, capability, evidence, or decision and the required unblocking condition. Select other eligible work. If there is none, expose the actual cause, retain progress, and wait for input or dependency recovery. Empty queues, blocked cycles, unscheduled outcomes, exhausted provider availability, and complete projects are distinct states. The planner handles discovered decomposition gaps automatically where existing user intent resolves them.

### 10. Client and tool surfaces

Add additive, feature-negotiated project/issue DTOs and operations to A3. Operations include capture/import design, propose/admit plan delta, inspect project/stage, query paginated issues, inspect exact issue, start/pause/resume project, propose issue revision, and retrieve an exact settlement receipt. Claims and successful settlements are host-only operations. Allocate canonical wire families/namespaces during implementation; do not reuse existing interview-stage or run-phase tags.

TUI and GUI share daemon projections and revision-fenced mutations. Both show why an issue is eligible or deferred and distinguish a task's successful closure from project completion. The GUI must expose its own functional project controls; displaying a notice to use a separate CLI-selected conversation is not parity. Reconnect preserves selected project/issue and resolves uncertain operations by identity.

Provide scriptable `peritus projects ...` and `peritus issues ...` operations for the same contracts. Proposed spellings are subject to the existing CLI argument conventions. Exported Markdown/JSON reports are portable views of authoritative state, not mutable files that workers can edit to bypass settlement. No external issue synchronization is required for the first delivery.

## Data and compatibility

### Logical records

| Record | Essential data |
| --- | --- |
| Project | Stable ID, owner/workspace binding, schema/revision, design reference, scope revision, required stage/outcome references, state, active transition, input provenance |
| Requirement | Stable ID, exact source/version/location, agreed text, preservation/exclusion status, outcome/issue references, evidence status |
| Stage session | Stage kind, generation, scope revision, entry conditions, context identity, active/completed/reopened state, accepted handoff/transition references |
| Subsystem outcome | Responsibility/boundary, requirement IDs, stage-specific acceptance issue references, required/optional status and its authority |
| Issue | Stable human-readable key and internal ID, revision, kind, parent, stage, requirement/source references, objective, contract, dependencies, derived readiness, lifecycle, blockers, historical settlements |
| Task attempt/claim | Issue/contract revision, claim epoch, worker/run/context identities, workspace reservation, effect references, safe-boundary state, observations |
| Task settlement | Exact governing binding, proposed/accepted outcome, code/artifact/evidence/review references, follow-ups, original operation ID and committed receipt |
| Dependency exception | Blocked issue, observed evidence, necessary remedy, permitted scope, governing revision and resulting unblocking evidence |
| Stage handoff | Accepted stage generation, code checkpoint, outcome and evidence references, open work, next-stage entry conditions |

An effort estimate is descriptive metadata, not an admission limit. Usage/duration measurements never decide that an otherwise active task exceeded its allowance.

Use a daemon-owned project journal backed by the existing C0 storage primitives, separate from legacy run JSON and from the bounded conversation record. Keep issue/requirement/attempt records individually addressable. Project and stage records contain indexed references and compact status, not embedded issue trees or transcripts. Artifact retention follows live project/result references. Page bodies and rebuild derived indexes incrementally so cold recovery and ordinary startup do not scan every historical artifact before IPC becomes available.

Reserve new versioned identities and frame namespaces through the repository's canonical protocol declarations. All mutations use expected revisions and idempotent operation IDs. Claim, result, and settlement identities are explicit rather than inferred from a title, wall clock, or model response. Reusing an operation ID with different input rejects the request.

Legacy conversations, goals, runs, module jobs, and interview stages retain their meaning. Existing single-run delivery remains available. Adopting an existing run or repository into a project is an explicit assessment/import operation; do not retroactively rewrite an active run's acceptance contract. New project data is not silently downgraded or overwritten by an older binary. Unsupported clients get a clear feature-unavailable response while ordinary supported operations continue.

Automatic closure of a task means its scoped outcome is accepted. It does not authorize a Git commit, push, merge, release, installation, or an unrelated external effect. Those follow existing user authorization and tool permissions. Task results may bind a worktree checkpoint rather than requiring one Git commit per tiny issue.

## Failure handling

| Condition | Required behavior |
| --- | --- |
| Model forgets issue closure or exits after doing the work | Host reconciles its exact attempt, obtains/validates the scoped result, and settles it or leaves an explicit unfinished state. No manual close ritual and no inferred success. |
| Crash before settlement commit | Recover the claim and published artifacts; inspect observed work and reconcile remaining acceptance. Do not blindly rerun effects. |
| Crash after commit or lost client acknowledgement | Resolve the original operation receipt and show the committed closure. Do not execute the completed task again. |
| Process/effect status is uncertain | Retain ownership constraints and enter reconciliation. Inspect owned handles, receipts, and actual state before retry or reassignment. Exact-once external effects are not promised where the external interface cannot support them. |
| Storage full, unwritable, or corrupt | Preserve the last committed state, show the real storage error, retain open work, and resume settlement after repair. Never report closure before its durable receipt exists. |
| Stale claim, scope, design, permission, or contract | Reject settlement against that binding, retain evidence as historical, and reconcile with the current issue. Never attach it to a different task. |
| Provider failure or context transport limit | Use existing provider recovery and context retrieval/compaction; retain the task. If no provider is available, show dependency recovery rather than budget exhaustion or successful completion. |
| Decomposition cycle, missing requirement, or empty actionable queue | Reject the conflicting plan delta or create a planner repair operation; preserve the prior usable plan and show unresolved scope. |
| Work drifts into later-stage polish | Prevent an ineligible claim; if discovered in a diff, return scoped remediation and retain the finding as deferred work. Preserve unrelated pre-existing user changes. |
| Handoff/context setup interrupted | Resume the committed transition using the same handoff; do not reopen the completed stage or reuse the previous stage's scratch context. |
| A task takes longer than estimated | Continue productive work; split additional outcomes at a safe boundary and retain progress. No elapsed-time cancellation. |
| Client disconnects or two clients act together | Execution stays daemon-owned. Reconnect reads authoritative receipts; revision and claim fences reject conflicts without changing the target issue. |

## Security considerations

The tracker narrows and records work; it grants no workspace, process, network, credential, or publication authority. Apply existing actor/workspace ownership, trust, live permissions, protected paths, process ownership, and cancellation controls before every effectful operation. An issue link or source path is an inert reference until an authorized reader resolves it.

Treat imported designs, repository documents, worker proposals, external issue text, and handoffs as untrusted content. They cannot install system instructions, mint host settlements, change stages, or expand authority. Planner tools can propose tracker mutations only. Clients cannot forge a host claim/closure receipt by posting equivalent JSON.

Preserve secrets through reference-based storage and redacted logs. Reports and tracker exports must not include provider reasoning, credentials, or unrelated private source bodies. Project scope/closure permissions and cross-user access are checked in the daemon, independent of what the UI displays.

The implementation starts with one mutating task per shared workspace. Additional concurrency requires independent workspace ownership and the same task-result freshness checks; speed must not come from concurrent agents overwriting one another's state. Existing available-resource controls remain appropriate and are separate from execution budgets.

## Verification

This is a documentation change; no builds, unit tests, or behavioral campaign are needed to write this design. The following is the required real-testing plan for implementation, following the [project policy](../README.md#real-testing) and real-testing skill.

Run the actual packaged or current built program with isolated state, real providers for planner/writer/reviewer behavior, real issue persistence, and disposable workspaces. Exercise the TUI and GUI controls themselves. Headless commands supplement those journeys, rather than replacing UI acceptance. Bound concurrent resource use to available resources and scope observations to meaningful scenarios; do not introduce a run-ending token/time/attempt budget to conduct the campaign.

Use a small application with multiple real subsystems to demonstrate the whole five-stage flow, plus a copy of an existing complex repository for brownfield adoption. Preserve the originals and active user daemons. Include an initially tempting Optimization task while required Coverage work is missing, an evidenced prerequisite exception, a no-change assessment issue, and an oversized assignment that must split.

Operate actual task completion and fault boundaries: omit any explicit close request, fail required behavior, lose an acknowledgement, interrupt between artifact publication and settlement, restart after a committed closure, and reconnect a second client. Terminate only owned disposable processes. Read back authoritative issue states, receipts, requirement coverage, source changes, actual outputs, and context/request manifests through their normal consumers.

Use reproducible generated sequences over the public tracker operations: independent/dependent issue creation, claim, result, failed acceptance, repair, split, block/unblock, pause/resume, scope revision, settlement replay, and reopen. Derive invariants independently from R01–R18: exclusive claims, dependency-respecting eligibility, no unsupported stage advancement, durable closure exactly once per accepted settlement, preserved requirements, and no false success. Concrete failing inputs and sequences belong in the existing `docs/testing/reproducers/` area during implementation, with expected behavior and an exact replay command.

Measure completion time, substantive outcomes, repeated inspection, task-splitting frequency, and full-project elapsed time during a sustained run. Compare against a documented baseline where available; do not claim a speedup merely because tasks are smaller or request counts fall. Inspect requests to prove stage/task separation rather than inferring it from role labels. Real evidence can establish observed behavior and correct bookkeeping; it cannot make semantic AI review infallible.

Map campaign evidence to A01–A20 and report PASS, FAIL, or BLOCKED with exact versions and remaining gaps. Completion requires the delivered implementation's real operations, not a simulated issue backend, source-only expectations, agent-generated unit tests, formal checks, or a generic green qualification suite.

## Implementation sequence

Each increment below is a delivery outline that the decomposition agent would further split into small issues. These are not claims that an increment itself takes a few minutes.

| Increment | Code and contract work | Data/compatibility impact | Completion evidence |
| --- | --- | --- | --- |
| 1. Tracker foundation | Add the pure project/requirement/issue domain and daemon C0 owner, revisioned operations, paginated read paths, claims and receipts. | New explicit schema/namespaces; no legacy run conversion. | Operate real issue creation, claim conflicts, restart, receipt replay, and large paged readback through a public consumer. |
| 2. Design decomposition | Connect selected guided/Markdown design versions to a read-only planner role; admit traceable plan deltas and dependency checks. | Immutable design and plan artifact references. | A01, A14, and requirement coverage on greenfield and repository-copy inputs. |
| 3. Scoped task loop and closure | Bind a real product run to an issue contract; update obligation selection, all role prompts, real-operation acceptance, review/fixer routing and automatic settlement. | Additive task binding and result schema; ordinary single-run path retained. | A04–A07 and crash/acknowledgement replays in A09–A11. No close command from worker or user. |
| 4. Stage/context coordinator | Add project maturity admission, evidence-backed exceptions, scoped context lineages, staged outcomes and automatic handoffs. | New stage generations/context namespaces, separate from interview/run/module stages. | A02–A03, A08, A13, and A19 with exact provider-view inspection. |
| 5. Human controls and steering | Add A3 projections, CLI/TUI and browser controls, exact issue details, plan/start/pause/resume and affected-revision replanning. | Feature negotiation and compatible read-only fallback for unsupported clients. | A12, A17–A18 exercised through both actual clients and reconnecting sessions. |
| 6. Recovery, scale, and delivery assessment | Complete retention/index recovery, process reconciliation, brownfield assessment, sustained-run observations and discovered-defect replay. | Validate retained schemas and older-reader refusal without data loss. | A09–A16 and A20 on delivered binaries, including ordinary coding/module regression journeys. |

Suggested integration owners are the existing daemon `product_control` and `product_run/workbench` areas; runner `execution`, `guided_design`, `local_context`, and `engineering_workflow`; A3 `workbench` and corresponding wire codecs; the TUI workbench area; and the web gateway/browser components listed under Current architecture. Proposed new source names must be added to their folder maps when implemented. Do not put the entire project feature into a root dispatcher or into the Deep Research plugin.

## Rollout and rollback

Deliver this as an explicit project workflow alongside existing single runs. Publish matching daemon/client support and an inspectable protocol capability. Existing `/design build` behavior remains compatible unless the user explicitly selects staged project execution; the Design screen can offer Break into staged tasks and Start project as clear actions.

Initial implementation and real testing use disposable state. Do not migrate an active user's long-running job into this model or restart its daemon merely to test the feature. Adoption assesses the existing workspace and records what is already delivered before scheduling new mutations.

Pause projects before disabling their execution capability. Rollback retains the project journal, issues, accepted results, and exported design/handoff artifacts. Older binaries must refuse unsupported project mutation rather than reinterpret records, silently discard issues, or start a task without a valid claim. Previously available single-run and module execution continue under their existing contracts where compatible.

## Open questions

No product decisions block beginning implementation from this design. The user has agreed to the five stages, separate stage contexts, narrowly scoped tasks, no execution budgets, real testing, a decomposition agent, stage-owned work, and automatic issue closure.

The following are proposed defaults that can be refined without weakening those decisions:

- The decomposition role uses the configured writer model; scoped independent review uses the configured review model. Do not add another mandatory provider setup step.
- Human issue keys use a project-local prefix and increasing display number, backed by stable opaque internal IDs. Final command labels follow existing client conventions.
- External tracker synchronization, optional concurrency across independent worktrees, and project templates can be added after the core workflow works. None is a prerequisite for local delivery.

## Out of scope

- Implementing this feature as part of authoring the design, starting project workers, or modifying active runs.
- Adding execution budgets, arbitrary retry allowances, fixed issue-count quotas, or elapsed-time task cancellation.
- Reintroducing unit-test or formal-verification gates as the definition of task completion.
- Replacing the model providers, removing the existing agent loop or review/fixer roles, or granting broader tool authority.
- Requiring GitHub/Crosslink/cloud accounts, implementing external issue synchronization, or turning every issue closure into a commit/push/release.
- Claiming that prompts alone enforce task scope, that AI review guarantees semantic correctness, or that smaller tasks guarantee faster delivery without measurement.
