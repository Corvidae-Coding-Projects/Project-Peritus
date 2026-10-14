# crates/runtime/peritus-sandbox-windows/src — navigation

## Responsibility

Windows 11 24H2 and Server 2025 native sandbox preparation.

## Files in this folder

| File | Role |
| --- | --- |
| [channels.rs](channels.rs) | Authorized proxy/secret preparation and exact protected-handle projection. |
| [config.rs](config.rs) | Immutable installation configuration and inert post-authorization preparations. |
| [conformance.rs](conformance.rs) | Runtime-neutral C2/C3 Windows conformance facts. |
| [descriptor.rs](descriptor.rs) | Probe-derived Windows backend descriptor. |
| [diagnostic.rs](diagnostic.rs) | Stable nonsensitive names for Windows resource diagnostics. |
| [error.rs](error.rs) | Stable Windows backend failures and recovery guidance. |
| [filesystem.rs](filesystem.rs) | Windows path identity, reparse checks, and exact temporary ACL policy. |
| [helper_process.rs](helper_process.rs) | Windows helper process entrypoint kept outside the binary composition root. |
| [identity.rs](identity.rs) | Deterministic nonsensitive helper, job, and token profile identities. |
| [lib.rs](lib.rs) | Windows 11 24H2 and Server 2025 native sandbox preparation. |
| [manifest.rs](manifest.rs) | Version-one bounded Windows helper manifest. |
| [network.rs](network.rs) | `AppContainer` deny-all and managed-filter proxy routing. |
| [network_filter.rs](network_filter.rs) | Session-owned Windows Filtering Platform policy lifecycle. |
| [observation.rs](observation.rs) | Bounded Windows-native lifecycle and enforcement observations. |
| [preparation.rs](preparation.rs) | Probe ownership, C2 binding validation, and authorized Windows preparation. |
| [probe.rs](probe.rs) | Deterministic Windows capability probe and support intersection. |
| [process.rs](process.rs) | Restricted token, `AppContainer`, Job Object, terminal, and handle policy. |
| [recovery.rs](recovery.rs) | Versioned native recovery record and exact ownership classification. |
| [refinement.rs](refinement.rs) | Named Windows refinement obligations for C3. |
| [release.rs](release.rs) | Final backend-owned Windows teardown evidence. |
| [resource.rs](resource.rs) | Dimension-specific Windows resource-control projection. |
| [runner.rs](runner.rs) | Structured helper launch and reserved pre-target failure categories. |
| [secret.rs](secret.rs) | Protected inherited handles for secret delivery. |
| [session.rs](session.rs) | C2-owned Windows native session lifecycle and teardown. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `NativeBindingFacts`, `native_binding_complete`, `lifecycle_transition_allowed`, `recovery_advance_allowed`, `teardown_complete`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-sandbox-windows`; follow each binary into its package implementation. |
| [channels/](channels/AGENTS.md) | Implementation pieces for `channels.rs`: Authorized proxy/secret preparation and exact protected-handle projection. |
| [filesystem/](filesystem/AGENTS.md) | Implementation pieces for `filesystem.rs`: Windows path identity, reparse checks, and exact temporary ACL policy. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Version-one bounded Windows helper manifest. |
| [native/](native/AGENTS.md) | Inventoried Windows FFI boundary for native probe, activation, and target creation. |
| [preparation/](preparation/AGENTS.md) | Implementation pieces for `preparation.rs`: Probe ownership, C2 binding validation, and authorized Windows preparation. |
| [probe/](probe/AGENTS.md) | Implementation pieces for `probe.rs`: Deterministic Windows capability probe and support intersection. |
| [process/](process/AGENTS.md) | Implementation pieces for `process.rs`: Restricted token, `AppContainer`, Job Object, terminal, and handle policy. |
| [session/](session/AGENTS.md) | Implementation pieces for `session.rs`: C2-owned Windows native session lifecycle and teardown. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
