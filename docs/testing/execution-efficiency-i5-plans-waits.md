# I5 completion: read-only plans and owned command waits

Status: complete for I5, phase six. **I0–I5 are complete; I6 remains incomplete.**
Architecture verdict: **ready** within the qualified scope below. Plans remain opt-in with
`local_context.read_only_plans = "sequential"`; `"disabled"` remains the default and rollback.
This completes the sequential plan/wait increment, not the whole efficiency design.

## Completed behavior

`inspection_plan` accepts the closed version-1 graph of ordinary list/search/read operations.
The host validates the complete inert graph before dispatch, binds the original parent and
current input, and independently admits each child under the actual model role. Earlier-result
JSON Pointers supply explicitly named arguments. Missing, failed or ambiguous results return
control for judgment. Literal and referenced values cannot supply the same argument. No mutation,
command, arbitrary code, parallel scheduler or model-authored permission grant is added.

Every completed child retains its original arguments, input revision, parent, dependencies,
canonical output bytes and digest. Bounded aggregate pages have explicit continuations; exact
child JSON remains available through `inspection_result_read`. Current source/path/read policy
applies to retrieval and continuation. Bounds are per operation and page, not lifetime task quotas.
Direct tools, complete catalogs, current grounding, candidate checks and independent acceptance
remain. Host children are recorded as host observations rather than fabricated provider calls.

`command_wait` observes the original owned C4/C2 handle until terminal state, selected stream
output, a control change requiring yield or its optional observation limit. Before-edit permits
proven reads; now/after-operation pauses end observation. Interactive output retains original
byte offsets and can be followed by `command_output` and exact `command_stdin`. A yielded wait
reports an unobserved command, preserves the handle, and neither starts nor implicitly cancels
the process. Unknown terminal evidence stays unknown. Passive checks do not reserve tool calls.
Runtime/record locks are released between polls; multithreaded Tokio uses `block_in_place`.

## Repairs found during qualification

| Saved evidence | Finding | Repair and observed result |
| --- | --- | --- |
| `i5-daemon-waits-first/wait-deny-process` | The real daemon accepted process-permission revocation, but an existing wait continued beyond the 30-second observation deadline. Its check covered read permission and input revision only. | The wait captures current effective permissions and the durable workspace permission revision, then checks both during observation. Revocation and revoke-then-restore now yield `permission_changed` without another process start. |
| `i5-before-edit-before` | A real before-edit pause stopped the second proven read although the domain policy permits reads. | Passive daemon control now respects BeforeEdit while actual mutation admission remains in the existing reservation boundary. |
| `i5-completion-zen/00-sequential` | DeepSeek supplied `path:""` alongside a reference supplying `path`. The host correctly rejected the whole plan, but the tool description omitted that grammar rule and the model fell back to direct tools. | Tool and argument guidance now explicitly require referenced fields to be absent, including empty/null placeholders. Rejection remains inert and actionable. The final DeepSeek run uses the valid two-child plan in two model requests. |

`ConversationView::permission_revision()` is an additive method with a default `None`; existing
static/legacy callers retain effective-permission checks. The daemon reads the existing monotonic
workspace policy ledger and fails closed if it cannot inspect it. Restoring the same permission
bits does not restore the old wait's revision. Requiring process permission merely to observe an
existing command was rejected as a repair: direct diagnostics and explicit recovery stay available.
This is not a new grant, persisted format, or cached authority decision.

The production changes in this phase are limited to `execution/types.rs`, daemon
`product_run/interaction/live.rs`, `developer_tools/executor/wait.rs`, and the plan `schema.rs`
and `validation.rs`, plus daemon `product_control/goal.rs`. The earlier sequential implementation is retained and qualified here.

## Actual daemon control and role evidence

All state is disposable and owned under
`/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`. A negotiated production IPC client
creates an isolated conversation, explicitly confirms its objective, starts the real goal, and
changes its actual input, permissions or lifecycle. Official Codex inference proposes the work.
GDB stops only the executing thread at the selected boundary; other daemon workers service IPC.
No provider response, tool output, permission receipt or goal counter is substituted.

| Campaign | Qualified observations |
| --- | --- |
| `i5-qualified-daemon-plans` | Four native plan cases: steer, revoke read, pause after operation and clear the goal after the first committed child. No second child dispatches; first-child provenance remains; host reservations are not charged by passive checks. |
| `i5-completion-waits` | Three active native-command cases: steering, pause and cancellation yield the original wait. Its process is still alive with its exact owned PID/cwd before the controller releases it. One start marker remains; tool counts do not grow through passive polling. |
| `i5-completion-authority` | Process revocation and revoke-then-restore yield without restarting or cancelling the original process. |
| `i5-completion-daemon-crash` | Kill the real daemon after the first child commits, then start a fresh daemon. One child and the original accounting survive. The run reports `RecoveryRequired`; no provider request or second child is automatically dispatched. The goal remains Active, not falsely completed. |
| `i5-final-reviewer` | Guidance-cohort independent reviewer plan: ordinary startup inspection, parent and child are all charged to Reviewer, Writer remains zero, and the child records `model_role=reviewer`. Pause prevents the next child. |
| `i5-final-authority` | Guidance-cohort revocation and restore cases repeat the wait regression. IPC readback proves the restored effective bits equal the original bits while the permission revision increases by two; the old wait still yields. |

The first four campaigns above use the permission-repair cohort, before the final plan-guidance
and rejection-text amendment. Its wait, permission API and exercised steering/after-operation/cancellation paths are
unchanged in the final code. The final reviewer/authority campaigns and both provider comparisons
exercise the guidance cohort. Each campaign retains its own executable identity. The last
daemon-only before-edit repair is qualified separately below; its Cargo rebuild reuses the
unchanged ProductRunner, agent and provider libraries used by the paired comparisons.

`i5-release-plans` runs the actual latest daemon: before-edit permits both children and returns
a completed plan, while after-operation pause stops at the first child. `i5-release-waits`
repeats blocking pause during a live command, and `i5-release-authority` repeats permission
restoration with policy readback. The actual mutation reservation gate is unchanged. This
preserves the selected pause boundary instead of treating all pauses as full stops.

## Interactive waiting, publication and restart

`i5-qualified-interactive-r2` uses actual Codex inference and one native interactive Python process.
The output wait spends **6,961 ms** waiting and returns the exact 17-byte terminal stream page.
`command_stdin` delivers exactly `NATIVE-INPUT-937154 Ω` plus Enter to that same handle. A second
wait runs normally for **2,076 ms** before the independent controller releases the original process.
It returns exit 0, complete retained terminal output, joined support tasks and tree cleanup.
There is exactly one start and no model `command_poll` or `run_command`. The native stdout and
the independently read input/start files agree. This is a wait/input interface pass; its
unqualified fixture is not presented as an Accepted product delivery.

This interactive campaign uses the permission-repair cohort. Later changes concern plan
guidance and the daemon BeforeEdit boundary; the exercised wait/input implementation is unchanged. The GDB boundary establishes entry, then resumes execution;
the two-second observation occurs after resume, rather than counting debugger suspension as
time spent waiting on the process. An earlier fixed-delay fixture finished before the second
wait arrived; that attempt is retained and excluded from active-terminal-wait evidence.

| Guidance-cohort replay | Result |
| --- | --- |
| `i5-final-plan-history` | Nine groups, including current admission, result references, no-match judgment, exact UTF-8 result pages, fresh-process continuation, changed sources, opaque/read restrictions, foreign scope, v3-reader refusal and disabled execution. Conflicting literal/reference fields reject before any child or reader upgrade. |
| `i5-final-plan-compiler` | The eight applicable groups also pass with decision-view compilation enabled. |
| `i5-final-plan-interruption` | Actual kills before/after child commit recover zero/one exact child respectively. A real SQLite append trigger refuses child publication. No later child runs; public source descriptors omit unpublished artifacts, and committed JSON is exactly retrievable. Recovery under disabled plans opens no unstarted `beta.txt` (strace). All committed roots and complete artifact dependencies verify. |
| `i5-final-owner-recovery` | Six real C4/C2 cases: running consumer death, kills before/after terminal projection, projection-write refusal, native process death, and four independent foreign-owner field mutations. Each retains one start and survives fresh-process readback without provider inference or launch replay. |

In owner recovery, a running owner's death remains **Indeterminate**, including after restart;
it is not inferred successful from a PID or fixture file. Verified C2 terminal results recover
exact stdout/stderr and exit 0 across both projection-kill boundaries and failed projection
publication. Actual process death reports **Failed**. Foreign ownership never becomes successful
execution. Projection publication is rebuildable; the durable native owner/terminal evidence is
authoritative. This does not claim that every killed running process can continue or has a
recoverable terminal result.

Additional permission-cohort regression checks pass: nine varied groups/264 API actions in
`i5-completion-varied`, eight v3 page-integrity/older-reader/rollback cases in
`i5-completion-integrity-corrected`, and the eight compiler groups in `i5-completion-compiler`.
These implementations were not changed by the final plan-guidance amendment.

## Qualified paired provider observations

The corpus uses seed **730924**, 24 source files, a unique literal search marker and an independent
exact answer at line 18. Both modes perform the same two inspections, leave every source byte
unchanged and return the exact expected JSON. Plans retain an additional accounted parent
operation; this is not free host work. Local state is fresh; vendor caches remain separately
reported. Full input includes cached input. Every normalized partition reconciles to canonical
C5 and cumulative accounting; unknown fields stay unknown.

| Qualified ProductRunner cohort | Direct | Sequential plans |
| --- | ---: | ---: |
| Codex `gpt-6.1-sol`, two alternating-order pairs: logical requests / adapter inferences | 6 / 6 | 4 / 4 |
| Codex total input | 92,598 | 65,228 |
| Codex cached-input subset | 15,872 | 15,872 |
| Codex output | 497 | 719 |
| Codex completion median, seconds | 25.840 | 18.046 |
| OpenCode 1.19.0/Zen `deepseek-v4.1-flash`, one pair: requests / inferences | 3 / 3 | 2 / 2 |
| Zen total input | 21,611 | 16,318 |
| Zen cached-input subset | 0 | 0 |
| Zen output including reasoning | 488 | 532 |
| Zen reasoning subset | 340 | 280 |
| Zen native total tokens | 22,099 | 16,850 |
| Zen completion time, seconds | 9.435 | 7.633 |

Input falls **29.6% on Codex** and **24.5% on Zen**. Each task uses two requests instead of three.
Observed completion time is lower in these small samples (Codex median 30.2%, Zen 19.1%). Some
control qualification overlapped; these are not isolated latency distributions or universal
speedup claims. Output is higher with plans. Native Codex reasoning/total counters and billing
are unavailable here, not zero; read-only time-to-first-useful-edit is not a measured outcome.
All six qualified comparison cases have zero format corrections. Providers are not pooled.

Exact rows, usage audits, unchanged-byte oracles and hashes are in `i5-final-codex-comparison`
and `i5-final-zen-comparison`, with aggregate `*-summary.json` files. Earlier paired runs and
the rejected pre-guidance DeepSeek plan are retained separately and excluded from these final
samples. The latter completed by direct fallback; it is not counted as a successful plan sample.

## Compatibility, reproduction and coverage

The existing checkpoint-v4 guard is committed before plan evidence; new host plan records remain
version 1. Existing v1–v3 state stays readable and ordinary runs keep their earlier floor until
needed. Supported I4 v5/v6 data remains readable. Disabling plan execution keeps current guarded
state and `inspection_result_read`, not an older checkpoint or an unsupported binary. No fsync,
artifact verification, CAS, completed-batch/request checkpoint, candidate or acceptance gate is
removed. The permission revision already exists in the policy ledger; no new migration is added.

The actual application cohort passes:

```sh
cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
cargo clippy --locked --jobs 2 -p peritus-context -p peritus-agent \
  -p peritus-product-runner -p peritus-daemon --all-targets --all-features -- -D warnings
cargo check --locked --workspace --all-targets --all-features --jobs 2
cargo fmt --all --check
git diff --check
```

Final application logs use the `i5-release-*` prefix. `i5-release-local-receipt.json` records
the last replay commands; `i5-final-local-receipt.json` records the guidance-cohort replay set.
The guidance-cohort consumers used by the comparisons and associated replays are:

| Consumer | SHA-256 |
| --- | --- |
| Context/tool API | `f7c35f163940bd6a97c9ef1a45d05ab1e98b88b0d91ff54e939c3cba01eb053a` |
| ProductRunner | `51f013388328a29ba4f102aeb967c252a7e483b7bcf372350f05f47337396d1f` |
| Negotiated IPC | `103703d16b306fa1600ec37643615d6fd3e6d526c444a2963cf672fc4009007b` |
| Native command owner | `cd9ff2d7034c4b297e69f835f96b347c357ec64858eb16b0a0ed746c45544df9` |

The latest daemon-repair consumers are `i5-release-reuse`
(`1855fa28569f35f33746ff05b495ff30fb9532abe29b45babdf8e997682e41d2`),
`i5-release-context` (`b4133c9c09e5827a652fce80ad04471e74babce87fc819760221c8ffe2754a55`),
`i5-release-control` (`64d5581e1d50c21839157b269bb70453b6c7d6c03b1e0aa3886106c29cb3478e`)
and `i5-release-owner` (`911a583eb85218d1b29b424917fee93eb884aa790ccc3bef827cc79451da8c5d`).
The last local plan/publication/owner campaigns repeat on this cohort in `i5-release-local-*`.

`i5-completion-source-identity.json` binds dirty source, manifests and actual binaries; Git HEAD
alone cannot identify this working tree. It is an identity inventory, not code coverage. Coverage
traces plan contracts/schema/validation, parent/child/continuation records, current source policy,
admission/accounting/activity, exact retrieval and reader guards; runtime start/poll/output/control,
native owners and projections; daemon input/goal/permission IPC; and the changed public trait's
workspace consumers. This is scoped I5 review and behavioral evidence, not a whole-codebase audit.

Setup-only failures are retained: missing IPC/brief prerequisites, a debugger selecting a running
thread, an uncommitted marker-file read, an underspecified fixture PID-file effect, a source-page
allocation above its configured bound, a duplicate debugger location, a missing integrity seed,
and an overly specific reviewer counter oracle that omitted permitted startup inspection. These
were corrected against the documented contracts; no host validation or expected safety outcome
was weakened. Incomplete attempts are not passing samples or zero-cost provider work.

See [the replay instructions](reproducers/incremental-context/README.md). Native Windows, other
runtime backends/models, a GUI journey, parallel read scheduling, and a live nonempty finding/fixer
cycle are not qualified here. They are not claimed as passing. All test processes and state are
fixture-owned; no saved user service, route or credential configuration was edited.

Branch `develop` and HEAD `d58ccbed426fba0a361a6533bb4ddb80726ba2ce` remain unchanged. Unrelated
work is preserved. No project commit, push, release or deployment was performed. I6 remains open.
