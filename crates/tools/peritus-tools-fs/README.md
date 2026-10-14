# peritus-tools-fs

## Folder guide

Authorized bounded filesystem tools for Peritus workspaces.

Active root-workspace package; declarations and targets are in [Cargo.toml](Cargo.toml).

Use [AGENTS.md](AGENTS.md) for file-level responsibilities, child modules, parent context, and nearby implementation owners.

### Local files

| File | Role |
| --- | --- |
| [Cargo.toml](Cargo.toml) | Cargo package/dependency/target declarations for `peritus-tools-fs`. |
| [README.md](README.md) | Human guide and operating/background notes for this folder; AGENTS.md provides the local navigation map. |

### Subfolders

| Folder | Role / next waypoint |
| --- | --- |
| [src/](src/AGENTS.md) | Bounded filesystem tools for immutable and authorized Peritus workspaces. |

Production filesystem tools for Peritus's model-facing C4 boundary.

The canonical catalog contains `fs.discover`, `fs.metadata`, `fs.read`, `fs.search`, `fs.create`,
`fs.write`, `fs.remove`, `fs.replace`, and `fs.patch`. Schemas are built from the bounded C4 schema
model and descriptors carry exact B1 operation classes, risk sets, limits, replay semantics, and
unique implementation identities.

Reads use a C1 `ReadOnlyWorkspace`. Paths are `WorkspacePath` values; traversal is deterministic,
bounded, protected metadata is filtered, and symlinks and special nodes are rejected without being
followed. Text is returned as UTF-8 and binary data as explicit base64 with an exact source digest.
Literal search has independent depth, entry, per-file, aggregate-byte, and match limits.

Mutations never call ambient filesystem write APIs. Every create, write, remove, replacement, or
multi-file patch compiles to an inert canonical `PatchSet`. `FsDispatcher` is the sole adapter that
can pass it to `WorkspaceGateway::apply_patch`, and its only effect entry consumes a router-created
`AuthorizedInvocation`. The dispatcher compares the validated C4 caller/target/digest binding with
the C1 authorization binding before effect. Successful `MutationOutcome` remains available for a
separately authorized Git candidate operation.

## Behavioral work

Use the [project real-testing policy](../../../README.md#real-testing) and the actual interfaces owned by this crate. The [local map](AGENTS.md) identifies entrypoints and implementation owners.
