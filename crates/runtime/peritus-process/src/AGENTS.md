# crates/runtime/peritus-process/src — navigation

## Responsibility

Owned native process backplane: authorization, dispatch, PTY/pipe I/O, cancellation, output retention, durable process state, recovery, and supervision.

## Files in this folder

| File | Role |
| --- | --- |
| [authorization.rs](authorization.rs) | Complete borrowed request for one target-owned execution decision. |
| [caller_binding.rs](caller_binding.rs) | Optional dependency-neutral parent invocation binding. |
| [cancellation.rs](cancellation.rs) | Closed cancellation and escalation vocabulary. |
| [command.rs](command.rs) | Checked structured executable and argument values. |
| [consumption.rs](consumption.rs) | Durable one-use authorization consumption and process registry. |
| [control.rs](control.rs) | Bounded non-owning process control and observation handle. |
| [environment.rs](environment.rs) | Deterministic clear-and-set child environments. |
| [error.rs](error.rs) | Stable process failures and recovery guidance. |
| [events.rs](events.rs) | Ordered bounded process events. |
| [gateway.rs](gateway.rs) | Sole target-owned authorization and execution effect gateway. |
| [identity.rs](identity.rs) | Complete owner lineage for one operating-system execution. |
| [intent.rs](intent.rs) | Exact opaque B3 execution-intent payload. |
| [io_policy.rs](io_policy.rs) | Checked pipe, PTY, input, output, and deadline policies. |
| [lib.rs](lib.rs) | Authorized process backplane; `ExecutionGateway` consumes authority into an `OwnedProcess`. |
| [lifecycle.rs](lifecycle.rs) | Deterministic owned-process lifecycle reducer. |
| [native.rs](native.rs) | Process-owned extension point for authorized native sandbox sessions. |
| [output.rs](output.rs) | Bounded stream accounting and retained output. |
| [plan.rs](plan.rs) | Complete canonical execution plan identity. |
| [plan_canonical.rs](plan_canonical.rs) | Canonical version-one execution-plan encoding. |
| [platform.rs](platform.rs) | Narrow operating-system launch adapters. |
| [public_api.rs](public_api.rs) | Stable crate-root exports for process consumers and sandbox backends. |
| [quiescence.rs](quiescence.rs) | Exact holder-quiescence inspection over the complete durable registry. |
| [recovery.rs](recovery.rs) | Durable process registry restart reconciliation. |
| [refinement.rs](refinement.rs) | Named C2 process-ownership and authority refinement obligations. |
| [registry_storage.rs](registry_storage.rs) | Protected registry filesystem layout and atomic record persistence. |
| [resource.rs](resource.rs) | Process resource ceilings and observations. |
| [result_api.rs](result_api.rs) | Public process result, recovery, quiescence, and resource observation surface. |
| [supervisor.rs](supervisor.rs) | Owned supervisor thread for process, I/O, cancellation, and terminal publication. |
| [terminal.rs](terminal.rs) | Deterministic terminal execution result. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `ExecutionAuthorityFacts`, `execution_authority_complete`, `terminal_accounting_valid`, `holder_quiescence_exact`, `NativePreparationFacts`. |
| [working_directory.rs](working_directory.rs) | Checked working-directory identity. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-process`; follow each binary into its package implementation. |
| [consumption/](consumption/AGENTS.md) | Implementation pieces for `consumption.rs`: Durable one-use authorization consumption and process registry. |
| [gateway/](gateway/AGENTS.md) | Implementation pieces for `gateway.rs`: Sole target-owned authorization and execution effect gateway. |
| [native/](native/AGENTS.md) | Implementation pieces for `native.rs`: Process-owned extension point for authorized native sandbox sessions. |
| [output/](output/AGENTS.md) | Implementation pieces for `output.rs`: Bounded stream accounting and retained output. |
| [plan/](plan/AGENTS.md) | Implementation pieces for `plan.rs`: Complete canonical execution plan identity. |
| [platform/](platform/AGENTS.md) | Implementation pieces for `platform.rs`: Narrow operating-system launch adapters. |
| [recovery/](recovery/AGENTS.md) | Implementation pieces for `recovery.rs`: Durable process registry restart reconciliation. |
| [supervisor/](supervisor/AGENTS.md) | Implementation pieces for `supervisor.rs`: Owned supervisor thread for process, I/O, cancellation, and terminal publication. |
| [terminal/](terminal/AGENTS.md) | Implementation pieces for `terminal.rs`: Deterministic terminal execution result. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
