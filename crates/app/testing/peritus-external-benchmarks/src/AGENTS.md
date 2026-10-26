# crates/app/testing/peritus-external-benchmarks/src — navigation

## Responsibility

Native, noninteractive external-benchmark entry point for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | External adapter handshake and pre-admission path preparation. |
| [agent.rs](agent.rs) | Shared real-product composition for admitted external benchmark invocations. |
| [args.rs](args.rs) | Strict benchmark-agent command grammar. |
| [candidate.rs](candidate.rs) | Exact read-only candidate capture at the external adapter boundary. |
| [command_runtime.rs](command_runtime.rs) | Protected command-runtime composition shared by external benchmark adapters. |
| [deadline.rs](deadline.rs) | Caller-derived `HarnessBench` work horizons with report-publication reserve. |
| [dispatch.rs](dispatch.rs) | Typed dispatch from external benchmark protocols into native Peritus responsibilities. |
| [error.rs](error.rs) | Stable failures at the external benchmark boundary. |
| [evidence.rs](evidence.rs) | Versioned terminal and retained invocation evidence. |
| [harness_results.rs](harness_results.rs) | Deterministic aggregation of retained `HarnessBench` campaign results. |
| [identity.rs](identity.rs) | Immutable identity for the native benchmark executable. |
| [lib.rs](lib.rs) | Native, noninteractive external-benchmark entry point for Peritus. |
| [process_entry.rs](process_entry.rs) | Process lifecycle and standard-stream handling for the native benchmark executable. |
| [providers.rs](providers.rs) | Credential-owning provider composition used by benchmark runs. |
| [publication.rs](publication.rs) | Atomic exactly-once report publication with a separately prepared recovery path. |
| [report_path.rs](report_path.rs) | Canonical containment and portable relative paths for retained benchmark reports. |
| [rubric.rs](rubric.rs) | Local text and image rubric completion through the selected credential-owning provider. |
| [session.rs](session.rs) | Durable external-benchmark conversation turns and per-turn trace identity. |
| [settlement.rs](settlement.rs) | Terminal settlement and unconditional publication guard. |
| [terminal_agent.rs](terminal_agent.rs) | Terminal-Bench admission wrapper around the shared native product composition. |
| [terminal_results.rs](terminal_results.rs) | Deterministic aggregation of retained Harbor Terminal-Bench results. |
| [workspace.rs](workspace.rs) | Exact benchmark workspace validation and baseline preparation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-external-benchmarks`; follow each binary into its package implementation. |
| [harness_results/](harness_results/AGENTS.md) | Implementation pieces for `harness_results.rs`: Deterministic aggregation of retained `HarnessBench` campaign results. |
| [terminal_results/](terminal_results/AGENTS.md) | Implementation pieces for `terminal_results.rs`: Deterministic aggregation of retained Harbor Terminal-Bench results. |
| [trace/](trace/AGENTS.md) | Projection of the durable D0 trace into external benchmark evidence. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
