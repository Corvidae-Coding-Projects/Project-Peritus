# crates/app/peritus-product-runner/src/execution/resume — navigation

## Responsibility

Implementation pieces for `resume.rs`: Digest-valid phase resume planning over retained product-run state.

## Files in this folder

| File | Role |
| --- | --- |
| [durable.rs](durable.rs) | Canonical persistence for the otherwise opaque product-run continuation. |
| [hashing.rs](hashing.rs) | Canonical length-delimited digests for retained knowledge inputs. |
| [knowledge.rs](knowledge.rs) | Role-specific, digest-bound knowledge retained for safe continuation. |

Owning module: [resume.rs](../resume.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../../AGENTS.md) · [Repository map](../../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
