# Native web tools navigation

## Responsibility

This C4 package owns native HTTP reading, retained source references and built-in Rust search and optional external SearXNG configuration. No model-provider or workspace-write authority lives here.

| File/folder | Responsibility |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Workspace package and HTTP/parser dependencies. |
| [README.md](README.md) | Human entry point. |
| [src/](src/AGENTS.md) | Native implementation and public consumer API. |

Product callers: [web tool adapter](../../app/peritus-product-runner/src/developer_tools/web.rs), [CLI](../../app/peritus-cli/src/web.rs). Browser rendering is owned by the existing computer runtime.
## Working in this area

Use [`Cargo.toml`](Cargo.toml) to confirm targets and dependency boundaries, then enter through the declared target entry point and trace public callers before changing behavior. Root workspace membership determines whether this package is active; nearby designs and retained evidence do not override the compiled interfaces.
