# crates/runtime/peritus-sandbox-linux/src — navigation

## Responsibility

Fail-closed Linux native sandbox preparation and lifecycle support.

## Files in this folder

| File | Role |
| --- | --- |
| [backend.rs](backend.rs) | Stable Linux backend identity and helper entry points. |
| [canonical.rs](canonical.rs) | Small canonical encoding helpers shared by manifests and recovery records. |
| [cgroup.rs](cgroup.rs) | Delegated cgroup-v2 resource ownership and cleanup. |
| [configuration.rs](configuration.rs) | Immutable installation configuration with inert resource owners. |
| [conformance.rs](conformance.rs) | Platform-neutral facts consumed by fresh-subject conformance adapters. |
| [descriptor.rs](descriptor.rs) | Descriptor construction from observed Linux support. |
| [error.rs](error.rs) | Stable Linux backend failures and recovery guidance. |
| [exec_status.rs](exec_status.rs) | Close-on-exec status channel distinguishing helper failure from target exit. |
| [filesystem.rs](filesystem.rs) | Deterministic mount and Landlock projection with protected-metadata dominance. |
| [lib.rs](lib.rs) | Fail-closed Linux native sandbox preparation and lifecycle support. |
| [manifest.rs](manifest.rs) | Versioned bounded helper manifest. |
| [network.rs](network.rs) | Network namespace and managed-proxy route projection. |
| [observation.rs](observation.rs) | Ordered digest-bound native lifecycle and dimension-specific resource facts. |
| [preparation.rs](preparation.rs) | Linux backend configuration, probe ownership, and authorized preparation adapter. |
| [preparation_validation.rs](preparation_validation.rs) | Validation of secret and proxy destinations before authorized native effects. |
| [probe.rs](probe.rs) | Runtime Linux capability probing. |
| [process.rs](process.rs) | Process-command projection helpers. |
| [proxy_preparation.rs](proxy_preparation.rs) | Authorized managed-proxy owner and protected-handle preparation. |
| [recovery.rs](recovery.rs) | Checksummed nonsensitive Linux runtime records and exact reopen classification. |
| [refinement.rs](refinement.rs) | Named Linux portions of the C3 formal obligations and their executable projection. |
| [resource.rs](resource.rs) | Dimension-specific Linux resource projection and helper rlimits. |
| [runner.rs](runner.rs) | Structured bubblewrap launch construction. |
| [secret.rs](secret.rs) | Exact checked secret-delivery bindings for process-owned protected handles. |
| [session.rs](session.rs) | C2-owned Linux native session lifecycle. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `NativeBindingFacts`, `native_binding_complete`, `support_covers`, `TeardownFacts`, `teardown_complete`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bin/](bin/AGENTS.md) | Executable entrypoints for `peritus-sandbox-linux`; follow each binary into its package implementation. |
| [filesystem/](filesystem/AGENTS.md) | Implementation pieces for `filesystem.rs`: Deterministic mount and Landlock projection with protected-metadata dominance. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Versioned bounded helper manifest. |
| [native/](native/AGENTS.md) | Linux-only safe native enforcement adapters. |
| [probe/](probe/AGENTS.md) | Implementation pieces for `probe.rs`: Runtime Linux capability probing. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
