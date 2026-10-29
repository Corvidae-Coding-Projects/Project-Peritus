# src — navigation

## Responsibility

Strict module metadata, atomic registry and digest-bound durable jobs.

## Files

| File | Role |
| --- | --- |
| [market.rs](market.rs) | Legacy executable catalog, additive source discovery, inert refresh, shared menu operations, and exact-run native-module presentation/progress state. |
| [market](market/AGENTS.md) | Versioned source-catalog snapshot and compatibility contract. |
| [lib.rs](lib.rs) | Public exports, typed errors and synced state publication. |
| [definition.rs](definition.rs) | Manifest/dependency validation, declared resources and package hashing. |
| [registry.rs](registry.rs) | Per-user bundled/installed selection, immutable install and enablement. |
| [jobs.rs](jobs.rs) | Exclusive ownership, ordered stage commits, exact output pages and integrity. |
| [preparation.rs](preparation.rs) | Credential-free public HTTPS fetch with public-address and byte/digest enforcement. |
| [source_build.rs](source_build.rs) | Durable source-build lifecycle, cancellation, interruption reconciliation and records. |
| [source_build](source_build/AGENTS.md) | Pinned source capture, bounded build-process execution and atomic disabled packaging. |

Read callers for exact behavior. This map adds no testing or approval gates.
## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.
