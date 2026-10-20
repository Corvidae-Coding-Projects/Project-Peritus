# peritus-product-runner

## Folder guide

Daemon-owned writer-reviewer-fixer product run coordinator for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [build.rs](build.rs) | Discovers and embeds every canonical skill package and resource. |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-product-runner`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [examples/](examples/AGENTS.md) | Direct native consumers of ProductRunner, including credentialed disposable-workspace examples; read each entrypoint before running it. |
| [src/](src/AGENTS.md) | The product execution coordinator: design, writer/reviewer/fixer turns, authorized tools, candidate/gate handling, local context, and final settlement. |

Canonical guidance lives in [repository skills](../../../skills/AGENTS.md).

Daemon-owned production composition for interactive Peritus coding runs. Every run begins with a
read-only, repository-grounded design pass that writes a durable Markdown task plan and
supplies it to all implementation turns. Source repositories use one startup grounding check per
run/workspace. A successful permitted workspace listing or file read completes that check;
the designer then has no further tools and returns its plan. Remaining source questions belong to
the next implementation slice, rather than an open-ended discovery phase. Explicit `kind = "artifact"` workspaces use a proportional deterministic Rust design built
from the exact durable conversation and a bounded sorted filesystem inventory, so a small
time-bound artifact task does not spend most of its deadline generating repetitive planning prose.
Both paths cover acceptance, findings, architecture, data flow, file plan, implementation slices,
verification, and explicit risks or non-goals. The adapter then joins:

- embedded production-engineering, architect, developer, reviewer, and real-testing workflow skills;
- the `peritus-agent` D0 provider/tool loop for real inspect, search, edit, command, test, and retry,
  with unread existing files protected from mutation and recoverable malformed, transport, and
  timeout terminals retried as fresh bounded provider attempts;
- `peritus-gates` changed-path planning, source readability, exact
  per-project format/compile/build/test/lint evidence, and native structural checks for changed
  CSV, JSON, YAML, and conventional SQLite migration artifacts;
- `peritus-review` typed policy-derived findings conserved through fixer and fresh-review cycles;
- `peritus-orchestrator` fail-closed E0 accept/fix decisions; and
- a durable task candidate, provider/tool trace, synced effect-receipt ledger, task-level summary,
  and explicit deliverable handoff consumed by the daemon.

Every coding role receives concise real-testing policy. The full
[real-testing skill](../../../skills/real-testing/SKILL.md) and all other guidance live in the
[canonical skill packages](../../../skills/README.md), discovered and embedded automatically by
`build.rs`. Agents discover packages and resource paths through `skill_list` and read relevant
pages through `skill_read`, without a per-skill Rust registry or copied instruction constants. Behavioral acceptance defaults to operating the actual program,
checking independent expectations, exploring relevant inputs and lifecycle transitions, and
preserving reproducible failures. User restrictions remain authoritative. The existing design,
check, reviewer, and fixer stages remain in place.

A changed file without an applicable executable check contract stops at check planning, retaining
the current candidate and naming the uncovered files. Missing check support is reported as a planning prerequisite
instead of entering speculative reviewer/fixer cycles. Existing supported checks and
candidate-failure repair continue normally; an unsupported candidate is not reported qualified.

The terminal contract is a verified `RunSettlement`, not success-or-error collapse. The runner
records a digest-bound checkpoint after each material mutation, successful verification command,
deterministic gate result, admitted review, and fixer cycle. Ordinary provider, repository, gate,
budget, cancellation, and recovery terminals therefore retain the strongest candidate and name its
remaining evidence. A fresh finalization observation clears a candidate that was completely
reverted, while an explicitly authorized external effect remains a valid candidate boundary.

Discard prepares source files and symlinks for every affected repository before publishing source,
index, HEAD, or repository-archive changes. Preimage materialization failures preserve the current
candidate and release the prepared Git locks, so fixing the filesystem condition permits retry.
Source staging stays outside paths being replaced and uses existing destination directories;
successful publication synchronizes new parents and replacement directory entries.

The explicit discard transaction API retains a checksummed, receipt-bound intent before restoration.
Inspection is inert. An explicit retry admits each owned source, staged-index entry, and nested HEAD
only at its captured candidate value or intended task preimage; unrelated staged entries survive.
Repository archives prove directory-move gaps, and a completed transaction returns its retained
history without touching later edits. Cleanup requires owned nonce markers, sealed preparation
contents, and recognized ordinary leaves. Incomplete or changed preparation is preserved and
reported; unknown native Git locks are never removed. This is explicit recovery across publication
boundaries, not an atomic swap of an entire workspace. Ordinary directory modes absent from legacy
task baselines cannot be reconstructed. Interrupted repository preparations remain beside the
independent backup when their safe cleanup cannot be established.

Continuation state is bound to the exact run, managed-workspace lineage, candidate digest,
conversation revision, and checkpoint sequence. S1 knowledge invalidation selects the first stale
phase; S2 obligation evidence participates in gates, reviewer input, acceptance, and fixer routing.
Provider recovery during review repeats review only, candidate changes repeat qualification, and a
conversation revision returns to design. When an embedding caller explicitly supplies a run
horizon, phase admission preserves a protected finalization reserve so an open-ended model turn
cannot consume the time needed to return an honest handoff.
The opaque continuation has a versioned durable encoding for the daemon. Restoring it rebuilds
role knowledge from the current conversation and deliberately reacquires effectful gate reports
instead of pretending an in-process runtime object survived restart.

Developer commands use the same command path as the rest of Peritus. `run_command` starts a
structured command through C4 and the daemon-owned C2 process store, waits for it, and returns a
bounded terminal result. Programs that need interaction or background progress use
`command_start`; the model can then poll the stable run-owned handle, write bounded terminal input,
resize the terminal, send a supported signal, cancel the owned process tree, or reconcile an
interrupted observation. Output is retained as C2/C4 artifacts and projected with both its opening
context and final diagnostics when it is too large. A terminal active result enters the same
review, acceptance, handoff, and command-created-file ownership ledgers as a finite command. The
product runner does not contain a second PTY implementation or call `std::process::Command` for
developer tools.

An active handle belongs to the live product run. `command_recover` reconciles interrupted polling
or control with the existing C4/C2 owner; it does not promise to recreate an arbitrary terminal
after a daemon process restart. Startup recovery separately reconciles durable C2 process state and
the durable product run remains explicitly recoverable.

New shell and local-compactor commands reserve a durable per-run ordinal before creating process
authority. Reopening the runtime, a failed start, and concurrent runtime instances cannot reuse a
reserved identity. The transactional SQLite allocator also skips existing authority and compactor
paths from older versions. Failed starts consume an ordinal; an unreadable or exhausted allocator
stops execution rather than resetting it. Effect receipts still determine whether an earlier
request may replay a result or requires reconciliation.

When the launcher supplied explicit automatic-failover consent, every designer, writer, reviewer,
and fixer invocation owns a deterministic provider cursor. The selected provider keeps its normal
bounded recovery first. Only then may the role advance to another configured tool-capable route;
media capability is checked against the current task. Safety, refusal, cancellation, raw ambiguous
transport, and normalized ambiguous acceptance never trigger a switch. Each accepted transition is
written to the append-only trace before its progress counter advances.

Startup grounding belongs to the run/workspace, not to each role or provider invocation. Its
completed marker survives retries, compaction, restarts, directory changes, and stale file receipts.
Those changes invalidate specific observations, not the completed startup phase. Editing an existing
file still requires its current read/hash; permissions and independent acceptance remain separate.
After the check, the designer receives an empty tool catalog with tool choice `none` and hands off.
Writer, fixer, and reviewer retain targeted tools for the actual task.

With local memory enabled, source delivery is tracked by workspace path, content digest, exact
line/byte coverage, and role-scoped archive handle. Unchanged ranges already present in the final
provider-visible context return successful `already_visible` reuse receipts without another body.
Missing archived pages restore automatically; overlapping requests read only their missing spans.
Truncated lines and reconstructed archive previews receive only their actual displayed coverage.
Unrelated edits preserve coverage. The workspace executor still checks permissions, normal paths,
OS access and the current digest before admitting reuse; changed sources refresh normally.
Independent review has its own archive and obtains its own evidence. Old observations without
exact coverage refresh conservatively, and unavailable reuse data falls back to an ordinary read.

The archive remains the durable source of truth; reuse adds no separate journal or run budget.
Current plans survive source changes as explicitly stale, non-authoritative progress, alongside
host-observed completed mutations. Current user requirements and fresh target evidence still govern
the next action. Efficiency events distinguish candidate reuse from applied receipts, restoration,
and delivery of missing spans and report actual returned bytes. The existing bounded inspection
warning remains supplementary; command polling is not inspection repetition.

The developer catalog also exposes bounded read-only reference listing and text reads. Their roots
come only from normal absolute paths in the current user-authored input, excluding provider replies
and host guidance. A reference path never enrolls workspace ownership, satisfies workspace mutation
grounding, or grants process access. Parent traversal and symbolic-link targets fail closed; missing
path components are matched to a unique case-insensitive sibling and return the actual on-disk
casing. An exact spelling wins, while case-colliding siblings fail as ambiguous instead of being
guessed.

Recovery progress is a content change, not a larger inspection scope: enrolling an unchanged or
absent path does not replenish retry allowances. Policy, cancellation, integrity, and ambiguous
terminals cannot be restarted merely because an earlier call changed a file. Malformed-response
recovery supplies explicit format guidance without replaying effects. Model requests, completed
tool calls, compactions, and accepted usage snapshots update shared accounting as they occur, so
later failures or cancellation cannot erase observed work; repeated usage snapshots are not added
twice. Workspace tool arguments must match the published closed schema before scope enrollment,
receipts, or execution. Missing-file reads return greenfield-aware corrective guidance.

Writable tool receipts bind a fresh host invocation identity, its ordered effect number, provider
call ID, and canonical request digest to `Started`, `Completed`, or `Ambiguous` state. The invocation
identity is shared with the model requests and retained before effects; a resumed writer or fixer
gets a new identity even at the same conversation revision, independently of local-memory mode.
Exact completed calls within the original identity
replay their bounded result. A command left in `Started` across restart is never launched again;
Peritus returns an explicit ambiguous observation so the agent or user can reconcile its effects.

The first workspace listing also grounds command execution in the current runtime rather than the
host's apparent CPU count. It reports logical and effective CPUs, the effective memory ceiling,
and a conservative recommended parallelism. Linux observations include cgroup-v2 limits. Command
execution applies that ceiling to common build ecosystems and returns a retryable error before
recognized Cargo, CMake, Make, or Ninja arguments can request a larger worker fan-out.

Independent review is a fresh D0 developer loop rather than a one-shot completion. It begins with
an observed workspace listing, reads the authoritative inputs and changed files through a
read-only tool executor, and only then admits the typed review. The executor rejects write, patch,
remove, and process calls even if a provider emits an undeclared tool name, so reviewer grounding
does not grant mutation authority. Malformed or ungrounded reviews receive their exact rejection
on a fresh bounded attempt. Each retry is counted and publishes a host recovery notice with the
reason and next attempt number. The ordinary conversation displays that notice separately from
provider text; the original response remains in the private trace. Invalid task-level reports
continue with corrective context until a valid grounded report, explicit user intervention, or a
concrete provider or host failure occurs.

The crate consumes already resolved provider and managed-workspace capabilities. It emits bounded
progress and deliverable evidence; it does not own UI, provider login, workspace trust, or Git
commit/export/discard authority. `Complete` is impossible for an empty or uncovered candidate, a
failed or missing exact-target command, or any unresolved policy blocker.

Normal interactive runs have no wall-clock product or provider-turn deadline. They continue until
the work reaches a terminal result, the user cancels it, or a concrete provider, transport,
resource, or integrity boundary fails. Benchmark and embedding callers may still select an explicit
positive horizon; only those bounded runs receive countdown prompts, reserve finalization
time, and return a typed elapsed-budget failure at the selected horizon. Per-command timeouts remain
independent so a stuck child process cannot consume an unbounded run.

One developer segment remains bounded to 48 logical turns and 512 tool calls. Reaching either
boundary continues in a fresh repository-grounded segment with completed effects and evidence
preserved. Malformed or ungrounded task-level terminals receive the exact rejection as corrective
context without a fixed retry ceiling. Fixer/reviewer cycles continue while acceptance remains
incomplete; repeated findings or unchanged fixes do not impose a terminal cycle count. Abrupt
daemon restarts resume interrupted goals from their persisted conversation, finding ledger, trace,
and unchanged managed worktree.

At every completed effect boundary, the runner also measures regular-file bytes beneath the
managed workspace and the harness process's resident memory through the host's ordinary process
accounting interface. Git object storage is excluded because it is repository history rather than
task growth; generated build trees remain included. The daemon persists and displays current
workspace size, positive growth from the run baseline, and the highest observed resident memory.
These measurements are observations, not cumulative stop thresholds. Human goals have no request,
tool, token, cost, memory, disk-growth, or wall-clock budget control. Explicitly bounded benchmark
or embedding callers retain their selected elapsed-time deadline. Per-operation protocol, output,
attachment, process, and concurrency bounds remain enforced.

Provider, filesystem, process, and Git effects remain ordinary Rust host adapters. In
`verus_only` builds the crate exposes the same daemon-facing boundary as a fail-closed total
implementation that cannot fabricate completion; the D0, D1, D2, and E0 decision cores remain in
their formally classified crates and the ordinary-API audit constrains the production adapter.

## Local working memory

Product writer/fixer invocations now default to durable local working memory across retries and
provider changes. Reviewers retain only their own exact observations under the existing
memory-excluding role policy. The bounded memory tools cannot satisfy fresh grounding or
completion gates. Legacy provider compaction is explicitly selectable, never a failure fallback.
See the [local working-memory guide](../../../docs/local-working-memory.md) for configuration,
storage, trace versions, read-only inspection, and optional native auxiliary inference.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.

Retained local context has no default observation, entry, text, link, or update-count quota.
Old saved policies are widened without changing semantic revisions or source identity.
Checkpoints store content-addressed working/source pages; current provider views select
retrievable records within the negotiated envelope. `context_read` returns cursor pages,
including `entry_json_fragment` pages when reference metadata itself exceeds a response.
Scope baselines use a transactional SQLite path index, import original JSON before-images,
and do not reject trees by descendant count, depth, or accumulated evidence bytes.
Trace recovery streams the suffix after a checked cursor; run metadata uses streaming atomic
JSON publication. Provider envelopes, positive caller-selected policies, native wire/storage
representation, filesystem authority, corruption checks and available machine resources remain
applicable. See [growth evidence](../../../docs/testing/retained-state-growth-real-2026-10-16.md).
