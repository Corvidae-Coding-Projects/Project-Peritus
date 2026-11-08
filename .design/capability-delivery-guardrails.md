# Construction coverage and functional completeness guardrails

Status: implemented in the working checkout on develop; verification results are recorded below.

The problem is a mismatch between local progress and the requested deliverable. A developer can
spend repeated work segments refining one subsystem while other required operations remain
unsupported. Previously every literal obligation also inherited the same gates-and-review Boolean.
Neither repeated source changes nor a green global suite demonstrated that each capability worked.

## Execution contract

The runner now owns a persistent capability plan. A read-only planning invocation decomposes the
original request and accepted domain-aware design into required outcomes, dependencies, source
anchors, positive criteria, and failure/preservation/recovery criteria. Every original public
obligation must be mapped. An independent read-only reviewer checks coverage before construction.
Existing working behavior must be represented and preserved; artifact-only work must not be used
to disguise an executable capability. A rejected plan carries its concrete review findings forward.

Construction chooses an undrafted capability whose prerequisite drafts exist. The writer returns
a typed draft handoff naming that capability, its implementation, and remaining implementation
debt. A handoff with remaining work keeps the capability open. An empty remaining list records a
construction claim, not a verified result. All required drafts precede whole-candidate verification.
Intermediate scaffolding is permitted without satisfying functional acceptance.

Construction uses renewable segments of twelve model turns and 128 tool calls. On segment exhaustion
the host rotates among ready unfinished capabilities. Three unfinished segments or incomplete
handoffs for the same assignment require a read-only decomposition pass. The original outcome
remains required, existing drafts remain, and newly admitted smaller prerequisites must unblock it.
This is a work-selection transition, not a task, token, elapsed-time, or retry budget. It never
terminates an authorized productive run because a segment ended.

During construction, role policy defers broad suites, exhaustive edge cases, optional refactoring,
hardening, and optimization. Cheap build/startup checks and necessary targeted checks are allowed.
Revisiting drafted work requires an observed build failure, downstream blocker, critical defect,
or current user steering. The host enforces stage transitions, selected handoff identity,
dependency eligibility, rotation, and forced decomposition. Semantic judgments about whether a
particular edit is polish or necessary integration still require the model and independent review;
this feature does not claim to sandbox arbitrary process edits by capability.

After draft coverage, the developer operates the complete system and repairs observed failures.
The independent final review returns a separate positive/failure observation and verdict for every
capability, plus an independent scope-completeness verdict. Executable behavior requires references
to actual successful host verification commands on the current workspace candidate. A legitimate
unsupported-operation response can prove a rejection requirement; it cannot prove a required
successful operation. No-op success, disconnected handlers, renamed TODOs, circular mocks, skipped
tests, and a generic green suite do not establish the requested behavior.

Literal obligation evidence is derived from the corresponding capability assessments. An omitted,
duplicate, unknown, fabricated, stale, or unproved capability cannot inherit another capability's
pass. Global gates and the ordinary conserved review ledger remain additional requirements.
Before handing behavior work to independent review, the writer must have a successful verification
bound to the final candidate. Cleanup that invalidates every verification returns to the writer
immediately with a concrete correction. Final capability review still checks relevance and coverage;
this preflight does not turn one generic success into proof of every capability.

## State and recovery

The run-owned trace sidecar `*.delivery.json` stores the proposed and admitted contracts, dependency
queue, draft claims, implementation handoffs, open debt, segment counters, and exact-candidate
independent assessments. It is outside model workspace mutation ownership. Publication uses a
private temporary file, file sync, atomic rename, and directory sync on Unix. Loading checks storage
bounds, schema/run/workspace identity, dependency validity, and draft membership. Symlinks are
rejected. Corrupt or incompatible state is reported rather than treated as completion.

The original task supplies stable public obligation identities; the accepted design carries domain
expansion and material amendments. Status questions do not erase the construction queue. A changed
accepted design invalidates the current plan/proofs and retains the preceding contract for
reconciliation. Current conversation and authority still govern every invocation. Qualification
also binds the live conversation revision so a new instruction cannot reuse an older verdict.

Successful verification commands carry the workspace fingerprint observed before and after actual
execution. Source-changing commands remain unbound. Async completions additionally retain their
original start fingerprint and original command request. Handle waits/recovery never authorize
redispatch. Legacy continuation commands have no such fingerprint and receive no capability credit.
The durable continuation schema advances to version 5 and still reads versions 2, 3, and 4.
Verdicts cite short host-issued references hashing the command and candidate, avoiding repeated
copies of large scripts in JSON. The reviewer receives the reference-to-command mapping and exact
retained process results. Standalone command executors retain their non-Git behavior; the production
coding writer explicitly enables candidate binding.

Restored runs with missing plans or unfinished draft coverage reenter writing. An older qualified
checkpoint cannot bypass the new delivery guard. Normal checking, final review, no-patch
investigations, reused qualification, and final acceptance consult the capability evidence.
Partial exports, user waits, cancellations, and unsuccessful settlement remain incomplete outcomes.
User permission restrictions and existing per-tool authority checks remain in their current owner.

Cold recovery without a full S5 continuation can restore the exact design underlying a previously
admitted capability contract. It requires the same run, workspace, task, policy, source lineage,
input revision, design bytes and admitted source digest. A merely Ready design without admitted work
cannot use this path. The original baseline research is historical intent, as it is for an S5
continuation; it is not fresh evidence of the implemented code. This avoids restarting design
because the run's own implementation changed baseline files. Writing, current source observations,
fresh gates and capability review still run. Changed scope, revised input or changed artifacts do
not inherit this continuation authority.

## Source coverage

This change traces the interactive product coordinator, not the separate journal AgentDriver.

| Responsibility | Inspected and changed owners |
| --- | --- |
| Scheduling, contract validation, storage | runner `delivery.rs` and `delivery/` |
| Writer/fixer preparation, retries, terminals | `turn.rs`, `turn/provider.rs`, `turn/terminal.rs`, `turn/delivery.rs`, writer/correction prompts |
| Reviewer requests, admission, findings | `reviewer_turn.rs`, `review.rs`, `execution/review_phase.rs` |
| Global gates, qualification, resumed acceptance | `execution.rs`, `execution/state.rs`, `execution/obligations.rs`, no-change investigation path |
| Accepted-design continuation after interruption | `delivery/store.rs`, `design.rs`, existing retained expansion/immutable artifact validation; negative cases in `design/research_tests.rs` |
| Real command provenance, async handles | developer tool executor, command/evidence/active owners |
| Durable continuation compatibility | `execution/resume/durable.rs` |
| Native behavioral verification | `examples/live_account_product_run.rs` |

## Verification

Focused regression replays cover dependency ordering, rotation, restart, retained unsupported-handler
debt, omitted/duplicate/fabricated/stale proofs, independent obligation outcomes, scope changes,
foreign-run state, dependency cycles, symlink state, and forced decomposition without scope loss.
The existing runner suite and strict all-target/all-feature Clippy are also run.

The real-provider campaign is reproducible with:

```sh
PERITUS_DELIVERY_CAMPAIGN=1 CARGO_BUILD_JOBS=2 cargo run -p peritus-product-runner --example live-account-product-run
```

It uses the authenticated Codex adapter with an explicit gpt-6.1-sol / High selection, real runner
tools, an isolated state store, and a disposable native Rust repository with an initially blanket
unsupported CLI. Independent native consumers check four successful CLI journeys, eight invalid-input
journeys, the preserved library operation, and the new library operation. The saved capability plan,
draft coverage, and independent verdicts must agree. This campaign incurs real model usage.

The standalone non-Git executor replay is:

```sh
python3 docs/testing/reproducers/capability-non-git-command.py
```

It compiles a native consumer against the actual library, invokes a real child in a plain folder,
and checks its expected exit 17, output and durable effect receipt.

Validation: 100 runner tests passed; strict workspace all-target/all-feature Clippy passed. The
credentialed six-capability campaign passed independent qualification after interrupted-process
recovery, with four separately checked CLI success journeys, eight failure journeys, and an actual
external library consumer. The standalone non-Git command replay passed. See
[the verification record](../docs/testing/capability-delivery-2026-11-08.md) for evidence and limits.
Neither a test count nor a build is a claim that semantic AI review is infallible. Pre/post source
fingerprints are observations, not a lock against arbitrary concurrent or transient source changes;
the reviewer must verify that commands exercised the actual affected interface and relevant criteria.
