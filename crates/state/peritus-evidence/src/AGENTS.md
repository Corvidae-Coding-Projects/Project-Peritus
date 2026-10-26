# crates/state/peritus-evidence/src — navigation

## Responsibility

Durable evidence provenance and portable bundle assembly for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [admission.rs](admission.rs) | Pure evidence admission planning over checked dependency observations. |
| [admission_work.rs](admission_work.rs) | Owned admission continuation, with exact retries before dependency work. |
| [artifact.rs](artifact.rs) | Bounded artifact hashing tied to the exact open file and durable metadata. |
| [canonical.rs](canonical.rs) | Canonical primitive encoders and bounded readers. |
| [causality.rs](causality.rs) | Canonical direct-cause links and ancestry validation. |
| [error.rs](error.rs) | Stable evidence failures and operator recovery actions. |
| [freshness.rs](freshness.rs) | Exact revision freshness and explicit invalidation decisions. |
| [invalidation.rs](invalidation.rs) | Explicit immutable evidence invalidations. |
| [lib.rs](lib.rs) | Durable evidence provenance and portable bundle assembly for Peritus. |
| [manifest.rs](manifest.rs) | Canonical portable evidence bundle manifest. |
| [provenance.rs](provenance.rs) | Exact committed journal provenance bound into evidence records. |
| [record.rs](record.rs) | Immutable evidence drafts, tags, and admitted records. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `bytes16_equal_from`, `bytes16_equal`, `revisions_equal`, `causal_position`, `bundle_section_transition`. |
| [work.rs](work.rs) | Cooperative ownership and cancellation for incremental evidence work. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [bundle/](bundle/AGENTS.md) | Deterministic portable bundle planning, streaming assembly, and offline verification. |
| [manifest/](manifest/AGENTS.md) | Implementation pieces for `manifest.rs`: Canonical portable evidence bundle manifest. |
| [sqlite/](sqlite/AGENTS.md) | Narrow shared-file `SQLite` evidence adapter. |

## Working in this area

Start at [`lib.rs`](lib.rs) for the module surface, then follow its declarations into the focused files and child waypoints above. Trace at least one caller and the relevant error or recovery path before moving responsibilities; the parent package map identifies the public boundary and adjacent owners.

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.
