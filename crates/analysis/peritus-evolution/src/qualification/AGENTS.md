# crates/analysis/peritus-evolution/src/qualification — navigation

## Responsibility

Implementation pieces for `qualification.rs`: Feature-gated production qualification for F0 commit and replay boundaries.

## Files in this folder

| File | Role |
| --- | --- |
| [approval.rs](approval.rs) | Authenticated approve-once prerequisite for atomic promotion qualification. |
| [evidence.rs](evidence.rs) | Canonical evidence, variant, assessment, and proposal fixture. |
| [harness.rs](harness.rs) | Exact baseline, candidate, and protected policy fixture. |
| [identity.rs](identity.rs) | Store-bound deterministic qualification identities. |
| [promotion.rs](promotion.rs) | Complete production promotion setup and atomic-boundary observation. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [harness/](harness/AGENTS.md) | Implementation pieces for `harness.rs`: Exact baseline, candidate, and protected policy fixture. |
| [promotion/](promotion/AGENTS.md) | Implementation pieces for `promotion.rs`: Complete production promotion setup and atomic-boundary observation. |

Owning module: [qualification.rs](../qualification.rs). Inspect that parent before changing the local pieces.

## Working in this area

Read the parent module that declares this directory, then use the file table to follow the narrow implementation path and its callers. Keep contracts, host effects, persistence, and presentation in their owning layers; directory proximity alone does not transfer authority.

Navigation: [Parent waypoint](../AGENTS.md) · [Package map](../../AGENTS.md) · [Repository map](../../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
