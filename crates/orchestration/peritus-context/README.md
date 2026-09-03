# peritus-context

Production C6 provenance graph, selection, compaction, token-budget, and render-plan contracts.

This H-class crate uses the canonical `peritus-codec` SHA-256 boundary to bind caller-supplied
context bytes. Its deterministic graph, selection, accounting, compaction-validation,
render-planning, and run-knowledge selection logic remains inside Verus modules and performs no
ambient I/O.

Role delta packets are bound back to exact context-node digests before rendering. The resulting
`ReusableContextSelection` retains the full run-knowledge provenance and distinguishes changed
authoritative facts, current references, and navigation-only summaries.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-context
```
