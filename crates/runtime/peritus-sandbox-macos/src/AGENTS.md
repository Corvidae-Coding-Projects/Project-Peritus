# crates/runtime/peritus-sandbox-macos/src — navigation

## Responsibility

MacOS 15+ native sandbox preparation.

## Files in this folder

| File | Role |
| --- | --- |
| [activation.rs](activation.rs) | Fixed-size activation acknowledgement bound to admitted native state. |
| [canonical.rs](canonical.rs) | Bounded canonical encoding helpers. |
| [descriptor.rs](descriptor.rs) | Backend descriptor derived only from runtime probe evidence. |
| [environment.rs](environment.rs) | Exact non-secret target environment projection. |
| [error.rs](error.rs) | Stable macOS backend errors. |
| [exec_status.rs](exec_status.rs) | Close-on-exec status channel distinguishing helper failure from target termination. |
| [filesystem.rs](filesystem.rs) | Deterministic Seatbelt profile compilation. |
| [helper.rs](helper.rs) | Direct-child helper protocol orchestration. |
| [lib.rs](lib.rs) | MacOS 15+ native sandbox preparation. |
| [manifest.rs](manifest.rs) | Versioned bounded helper manifest and activation-record protocol. |
| [network.rs](network.rs) | Managed-proxy-only network projection. |
| [observation.rs](observation.rs) | MacOS-specific bounded lifecycle and control observations. |
| [preparation.rs](preparation.rs) | Fail-closed native preparation and C2 adapter. |
| [probe.rs](probe.rs) | Runtime capability probing with a deterministic, testable evidence projection. |
| [process.rs](process.rs) | C2-owned process-group, PTY, and inherited-descriptor mapping. |
| [recovery.rs](recovery.rs) | Version-one durable runtime identity and cleanup records. |
| [refinement.rs](refinement.rs) | Named macOS portions of the C3 formal obligations. |
| [resource.rs](resource.rs) | Dimension-specific macOS resource projection. |
| [resource_monitor.rs](resource_monitor.rs) | Bounded macOS process-group and workspace resource sampling. |
| [runner.rs](runner.rs) | Reserved helper exits and target handoff. |
| [secret.rs](secret.rs) | Protected secret-handle metadata and exact checked destinations. |
| [session.rs](session.rs) | Supervised native session lifecycle. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `NativeBindingFacts`, `native_binding_complete`, `deny_dominant`, `TeardownFacts`, `teardown_complete`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-sandbox-macos`; follow each binary into its package implementation. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Versioned bounded helper manifest and activation-record protocol. |
| [preparation/](preparation/AGENTS.md) | Implementation pieces for `preparation.rs`: Fail-closed native preparation and C2 adapter. |
| [probe/](probe/AGENTS.md) | Implementation pieces for `probe.rs`: Runtime capability probing with a deterministic, testable evidence projection. |
| [recovery/](recovery/AGENTS.md) | Implementation pieces for `recovery.rs`: Version-one durable runtime identity and cleanup records. |
| [resource_monitor/](resource_monitor/AGENTS.md) | Implementation pieces for `resource_monitor.rs`: Bounded macOS process-group and workspace resource sampling. |
| [runner/](runner/AGENTS.md) | Implementation pieces for `runner.rs`: Reserved helper exits and target handoff. |
| [session/](session/AGENTS.md) | Implementation pieces for `session.rs`: Supervised native session lifecycle. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
