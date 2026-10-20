# crates/observe/peritus-trace/src — navigation

## Responsibility

Durable causal tracing and redaction boundary for Peritus.

## Files in this folder

| File | Role |
| --- | --- |
| [developer.rs](developer.rs) | Stable developer trace frames, errors, and streaming read-only access. |
| [binding.rs](binding.rs) | Immutable binding from observations to existing Peritus domain identities. |
| [codec.rs](codec.rs) | Canonical family-60 observation encoding and decoding. |
| [error.rs](error.rs) | Stable redaction-safe trace failure vocabulary. |
| [format.rs](format.rs) | Stable trace wire-format identity. |
| [identity.rs](identity.rs) | OpenTelemetry-compatible nonzero trace and span identities. |
| [lib.rs](lib.rs) | Durable causal tracing and redaction boundary for Peritus. |
| [recovery.rs](recovery.rs) | Restart recovery and deterministic C0 rebuild entry points. |
| [redaction.rs](redaction.rs) | Sensitive-value consumption and encrypted artifact-vault references. |
| [storage.rs](storage.rs) | C0 compare-and-append storage for canonical trace observations. |
| [verified.rs](verified.rs) | Executable invariant/refinement predicates for the local domain. Entry names: `CausalFacts`, `causal_integrity`, `next_sequence`, `exact_duplicate`, `NonAuthorityFacts`. |

## Child areas

| Folder | Role / next waypoint |
| --- | --- |
| [domain/](domain/AGENTS.md) | Closed content-free trace observation domain. |
| [projection/](projection/AGENTS.md) | Pure causal trace projection over checked C0 records. |

Navigation: [Parent waypoint](../AGENTS.md) · [Repository map](../../../../AGENTS.md).

Keep this waypoint aligned when local files or child responsibilities change. Descriptions are orientation, not a substitute for reading the current implementation.

[Developer trace reader](developer/AGENTS.md) owns the selected-frame iterator.
