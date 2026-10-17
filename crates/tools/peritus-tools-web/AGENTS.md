# Native web tools navigation

This C4 package owns native HTTP reading, retained source references and SearXNG search/setup. No model-provider or workspace-write authority lives here.

| File/folder | Responsibility |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Workspace package and HTTP/parser dependencies. |
| [README.md](README.md) | Human entry point. |
| [src/](src/AGENTS.md) | Native implementation and public consumer API. |

Product callers: [web tool adapter](../../app/peritus-product-runner/src/developer_tools/web.rs), [CLI](../../app/peritus-cli/src/web.rs). Browser rendering is owned by the existing computer runtime.
