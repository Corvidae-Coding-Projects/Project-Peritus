# peritus-run-knowledge

## Folder guide

digest-bound run knowledge and delta planning for Peritus.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-run-knowledge`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Digest-bound run knowledge and role-delta planning for Peritus. |

`peritus-run-knowledge` is Peritus's pure, verified model for retaining grounded knowledge across
writer, reviewer, and fixer rounds. It binds every section to an exact workspace candidate, source
digests, conversation revision, role, and creation sequence.

The crate decides which prior sections remain reusable, which become stale, and whether a role
packet must carry a changed fact or may carry a reference to current knowledge. Model-authored and
compacted summaries remain navigation-only and cannot satisfy authoritative evidence requirements.

It performs no filesystem reads, hashing, model calls, persistence, or prompt rendering. Callers
supply observed digests and effectful `peritus-context` code binds selected sections to exact
context nodes.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
