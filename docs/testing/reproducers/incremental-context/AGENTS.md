# Incremental-context replay waypoint

## Responsibility

This folder operates production APIs to check reuse without changing source/permission semantics.
It is separate from Cargo tests. Build/relink after production changes; use disposable disk-backed
workspaces, keep independent expected values, and preserve actual failure inputs and results.

| File | Role |
| --- | --- |
| [README.md](README.md) | Build, replay, scope, measurement and storage guidance. |
| [link.py](link.py) | Links the consumer against Cargo's actual freshly built dependency cohort. |
| [reuse-consumer.rs](reuse-consumer.rs) | Line-oriented public workspace-tool/context consumer; includes shared setup from the checkpoint-cache consumer. |
| [replay.py](replay.py) | Seeded pages, source/access changes, current policy, pending state and invocation lifecycle checks. |
| [pressure.py](pressure.py) | Exceeds decode retention, revisits sources, and rejects repeated loads of omitted/obsolete history. |
| [observed-results.json](observed-results.json) | Small final measured results; raw source/trace corpora stay outside the repository. |
| [compare.py](compare.py) | Exact pre/post API-output comparison with Linux logical IO measurements. |

Production owners: `peritus-product-runner` inspection and local-context assembly; the composable
estimator belongs to `peritus-agent`. See [parent waypoint](../AGENTS.md).
## Working in this area

Start with [`README.md`](README.md) to recover prerequisites, disposable-state rules, and the independent oracle before running anything. Keep generated binaries and mutable run state outside the repository, and treat a replay as evidence for the exact observed build and scenario rather than universal product coverage.
