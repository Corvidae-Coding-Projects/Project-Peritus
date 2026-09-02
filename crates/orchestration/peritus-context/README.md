# peritus-context

Production C6 provenance graph, selection, compaction, token-budget, and render-plan contracts.

This H-class crate uses the canonical `peritus-codec` SHA-256 boundary to bind caller-supplied
context bytes. Its deterministic graph, selection, accounting, compaction-validation, and
render-planning logic remains inside Verus modules and performs no ambient I/O.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-context
```
