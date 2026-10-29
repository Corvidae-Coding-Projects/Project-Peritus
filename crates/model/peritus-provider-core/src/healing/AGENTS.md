# crates/model/peritus-provider-core/src/healing — navigation

## Responsibility

Implementation pieces for `healing.rs`: Bounded syntax-only repair of complete model-authored JSON, never transport framing.

## Files in this folder

| File | Role |
| --- | --- |
| [buffer.rs](buffer.rs) | Preserve valid provider fragments verbatim while delaying admission until argument close. |
| [syntax.rs](syntax.rs) | Conservative byte scanner: strings are opaque and only complete containers are extracted. |

Owning module: [healing.rs](../healing.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
