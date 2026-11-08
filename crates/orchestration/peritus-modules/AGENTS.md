# peritus-modules — navigation

## Responsibility

Declarative module package and job domain. The host owns inference, tools and authority.

## Files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Package and dependency declarations. |
| [README.md](README.md) | Usage, package format, lifecycle and boundaries. |

## Child areas

| Folder | Role |
| --- | --- |
| [src/](src/AGENTS.md) | Definition, registry and checkpoint implementations. |
| [examples/inspect_predecessor_integrity.rs](examples/inspect_predecessor_integrity.rs) | Public inspection replay for damaged predecessor outputs and recovery. |
| [examples/pending_stage.rs](examples/pending_stage.rs) | Public manifest-bound stage replay for declared inputs, paging, publication, and restart. |
| [bundled/](bundled/AGENTS.md) | Built-in workflow manifests using the same contract as installed packages. |

Read callers for exact behavior. This map adds no testing or approval gates.
## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.
