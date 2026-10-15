# peritus-debugger

## Folder guide

Durable evidence-linked trace diagnosis and harness analysis for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-debugger`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Durable evidence-linked trace diagnosis for Peritus. |

`peritus-debugger` is the durable E2 boundary for evidence-linked agent diagnosis. It freezes exact
run, attempt, revision, environment, workspace, harness, query, and C7 trace bindings; builds
deterministic timelines, causal alternatives, cross-run patterns, component correlations, and
diagnostic health summaries; validates every citation; and publishes bounded immutable reports
through C0 artifacts and evidence.

Optional C5 model analysis is provider-neutral and inert until strict schema, bounds, provenance,
and citation validation succeeds. The crate cannot mutate E1 harnesses, run E3 evaluations, make
F0 promotion decisions, grant authority, waive findings, accept runs, or expose raw-vault bytes by
default.

The complete frozen contract is in [the E2 design](../../../.design/e2-debugger.md).

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
