# crates/app/peritus-daemon/src/cli — navigation

## Responsibility

Implementation pieces for `cli.rs`: Process-facing command-line composition for `peritusd`.

## Files in this folder

| File | Role |
| --- | --- |
| [arguments.rs](arguments.rs) | Closed command-line vocabulary and exact argument parsing. |
| [blob_corruption.rs](blob_corruption.rs) | CLI handlers for artifact-corruption containment qualification. |
| [context.rs](context.rs) | Read-only inspection of a role-scoped published local context. |
| [daemon_lifecycle.rs](daemon_lifecycle.rs) | Closed process-facing daemon lifecycle kill and replay commands. |
| [dependency.rs](dependency.rs) | Closed process-facing dependency failure qualification commands. |
| [disk.rs](disk.rs) | Artifact quota qualification command rendering. |
| [evidence_corruption.rs](evidence_corruption.rs) | Acceptance-evidence corruption qualification command rendering. |
| [gate.rs](gate.rs) | Fixed CLI handlers for H1 gate commit crash qualification. |
| [journal_corruption.rs](journal_corruption.rs) | CLI handlers for authoritative-journal corruption and fail-closed startup qualification. |
| [lease.rs](lease.rs) | Fixed CLI handlers for H1 lease commit crash qualification. |
| [patch.rs](patch.rs) | Fixed CLI handlers for H1 patch commit crash qualification. |
| [process_watchdog.rs](process_watchdog.rs) | Hidden independent owner-death mode for exact daemon-launched process sessions. |
| [projection.rs](projection.rs) | CLI handlers for controlled projection corruption and startup repair qualification. |
| [promotion.rs](promotion.rs) | Fixed CLI handlers for H1 atomic promotion crash qualification. |
| [promotion_evidence_corruption.rs](promotion_evidence_corruption.rs) | Harness-promotion evidence corruption qualification command rendering. |
| [reboot.rs](reboot.rs) | Disposable-host reboot qualification command rendering. |
| [server.rs](server.rs) | Tokio runtime ownership for the long-lived daemon command. |
| [snapshot.rs](snapshot.rs) | Fixed CLI handlers for H1 retained Git snapshot crash qualification. |
| [snapshot_corruption.rs](snapshot_corruption.rs) | CLI handlers for retained Git snapshot corruption containment qualification. |
| [usage.rs](usage.rs) | Stable human-readable command synopsis. |

Owning module: [cli.rs](../cli.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
