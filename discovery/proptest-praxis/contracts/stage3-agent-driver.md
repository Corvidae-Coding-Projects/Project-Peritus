# Stage 3: durable AgentDriver process-loss campaign

This lane exercises the public `peritus_agent::AgentDriver` against actual file-backed `SqliteJournal` owners. It is separate from the interactive daemon `ProductRunService` lane. Current source search finds no product caller of `AgentDriver::start`; this campaign qualifies the published durable adapter seam, not an inferred product integration.

## Grounded owners and boundaries

Read `peritus-agent` package/source/runtime/driver waypoints, its active manifest, `runtime/driver.rs`, `runtime/driver/tool_steps.rs`, `runtime/durability.rs`, `runtime/model.rs`, `runtime/tools.rs`, `runtime/budget.rs`, `command.rs`, `identity.rs`, `limits.rs`, `phase.rs`, `state.rs`, `tools.rs`, `tools/batch.rs`, `reducer.rs`, `completion.rs`, and `control.rs`. Read the journal and tool package waypoints, journal SQLite connection/settings ownership, public replay access, tool protocol descriptor/call/schema/prepared/result/control constructors, router registry/exposure/dispatch/dispatcher interfaces, provider-core adapter/owned-stream/error boundaries, and C5 request/profile/event constructors.

`start_model_once` commits model-attempt intent before provider startup. `drive_model_once` commits one canonical C5 envelope before acknowledging the live stream. `dispatch_tool_once` commits `ToolDispatched` before invoking the real C4 router. `record_tool_observation_once` durably records active/completed observations. `restore` validates event/checkpoint agreement and intentionally invents no model session or tool owner. `classify_lost_tool_once` closes a dispatched/active slot as `Indeterminate` without invoking a dispatcher.

C4 authority fixtures adapt `developer_tools/command_runtime/{contract,identity,journal,kernel,authority}.rs`: public specification, kernel, policy, budget, and journal APIs produce real committed kernel/capability/budget/authority-epoch evidence. These constructors provide execution inputs; their outputs never define expected recovery state or effect counts. No reducer, verified predicate, production replay, or canonical digest helper is used as the oracle. `AgentDriver::restore` necessarily invokes its production replay implementation, which is the system under test.

## Executed histories and independent oracle

`Case` chooses one to three ordered ordinary tools, zero to two provider startup failures with explicit safe-new retries, optional pause/checked-resume history, completed prefix before interruption, one of eleven cuts, terminal completion/cancellation/failure, and repeated reopening. Generated cuts include provider-start intent, provider prefix, authorized unstarted tools, durable dispatch before effect, fsynced effect before return, active tool, completed tool, result recording, results committed, completion proposed, and terminal committed.

Every check spawns its own executable in a fresh disposable directory. The child uses the real driver, real SQLite journal, a recording `ModelProvider`/`ModelStream`, actual `AgentBudgetReservation`, real C4 `ToolBatchCoordinator`/`ToolRouter`, and a recording ordinary `ToolDispatcher`. The dispatcher increments a separate append-only fsynced effect log only after C4 supplies its move-only invocation permit. Provider startup similarly appends a fsynced external log. A complete ready marker is published by rename after its file is synced. The parent terminates only its owned child after this marker, waits for its exit, then creates a new driver from the durable store.

The oracle is an independent table from cut semantics to phase, provider-event cursor, context-cycle count, tool-call count, retry count, completed prefix, uncertain ordinal, and exact external effect history. It also checks each surviving slot's completed/uncertain/unstarted status. Consecutive reopen operations must preserve exact canonical bytes and must produce no provider or tool effect. Actual event-chain cardinality and transition counters must match. Attempts to redispatch completed durable slots, classify one lost slot twice, resume without completed recovery checks, or add commands to a terminal driver must reject without changing state.

After provider ownership loss, an explicit `ProviderRetryScheduled` must have no provider effect; only an explicit new `start_model_once` may add one recorded attempt. This retry drains and settles before a completion proposal. After tool ownership loss, an explicit classification must produce `Indeterminate`, even when the external effect definitely happened. For a single-call batch, actual result recording must preserve `has_unresolved_indeterminate` across finalization and reopen. Multi-call interrupted batches retain their individual indeterminate result while authorized unstarted siblings remain visible.

Cancellation is checked as an aggregate phase transition. `control.rs` explicitly leaves external termination/reconciliation to the caller; this campaign does not call cancellation phase completion proof of provider or OS-process termination.

## Calibration

`calibration/stage3-agent_driver.patch` changes the result of `indeterminate_result` from `Indeterminate` to `Succeeded`. The calibration witness is one tool killed from inside its ordinary dispatcher immediately after its effect log is synced, before a terminal tool result can return. Restoring and classifying this dispatched slot must never report success. Baseline explicit and generated cases must pass; mutant explicit and generated cases must fail with persisted first/minimized witnesses; the exact original production bytes must be restored, rebuilt, and both witnesses replayed successfully. The worker has not applied this mutation.

## Metrics

Metrics count owned-process crashes, SQLite reopens, provider/tool effects, retained completed effects, classified lost tools, startup retries, explicit restart retries, rejected replays, durable transitions, and each reached cut. They report observed bounded scenarios, not coverage percentages or proof of all interleavings.

## Scope and remaining gaps

The B1 provider port uses the actual in-memory `BudgetLedger`, with a distinct budget account for each provider attempt. Each fresh account therefore receives an initial-attempt Begin; D0 retry scheduling/counting is tested separately. B1 retry-lineage accounting and durable commit recovery are outside this lane. C4 authority uses actual committed SQLite receipts, but no workspace/process/network/secret lower gateway is launched: the ordinary recording tool's only effect is its disposable external log. Active owner methods exist to create an actual active C4 handle; interruption loses that handle, and the campaign verifies conservative D0 recovery rather than pretending to recover an OS process.

Interruption happens at explicit externally observable cuts, not every SQLite instruction or power-loss/cache-flush boundary. Journal corruption, disk-full, concurrent writer races, exactly resumed provider cursors, action-denial histories, and success continuation of a partially executed multi-tool batch need separate campaigns. Recovery currently cannot reattach prepared tools outside `ProposedToolCalls`; this lane preserves and terminates interrupted execution rather than fabricating live owners for authorized siblings. Cross-platform child termination and SQLite filesystem semantics must be qualified on their actual platform before claiming them there.

No build, search, replay, or successful calibration cycle is claimed until the parent records final-harness evidence. Only source formatting and whitespace checks have run in the worker.
