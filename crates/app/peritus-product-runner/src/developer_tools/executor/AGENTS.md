# crates/app/peritus-product-runner/src/developer_tools/executor — navigation

## Responsibility

Concrete workspace executor lifecycle, construction, command ownership, mutations, checkpoints, and progress feedback.

## Files in this folder

| File | Role |
| --- | --- |
| [active.rs](active.rs) | Evidence and workspace ownership for commands completed through an active handle. |
| [checkpoint_observer.rs](checkpoint_observer.rs) | Candidate-checkpoint classification for accepted developer tool effects. |
| [command.rs](command.rs) | Structured command execution through the shared C4 router and C2 process owner. |
| [construction.rs](construction.rs) | Read-only and explicitly writable filesystem/tool runtime construction. |
| [effects.rs](effects.rs) | Effect receipts, delivery progress, and bounded filesystem mutations. |
| [in_place.rs](in_place.rs) | Enroll task files or bounded directory subtrees before effects in non-Git work. |
| [inspection_progress.rs](inspection_progress.rs) | Bounded, content-based detection of an unchanged inspection cycle. |
| [read_reuse.rs](read_reuse.rs) | Admit cached/restored read plans through normal permissions, path checks, current file version, and grounding. |

Owning module: [executor.rs](../executor.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
