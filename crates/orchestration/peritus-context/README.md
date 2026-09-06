# peritus-context

Production C6 provenance graph, selection, compaction, token-budget, and render-plan contracts.

This H-class crate uses the canonical `peritus-codec` SHA-256 boundary to bind caller-supplied
context bytes. Its deterministic graph, selection, accounting, compaction-validation,
render-planning, and run-knowledge selection logic remains inside Verus modules and performs no
ambient I/O.

Role delta packets are bound back to exact context-node digests before rendering. The resulting
`ReusableContextSelection` retains the full run-knowledge provenance and distinguishes changed
authoritative facts, current references, and navigation-only summaries.

The `working` module adds bounded task-local investigation state: provider-independent scope,
exact artifact-range observation handles, source-backed atomic updates, canonical dependency
validation, and transitive stale/superseded status. Every working entry is non-authoritative.
Hosts supply verified, redacted inputs; these reducers perform no storage or provider calls.
This engine is not yet wired into the product. Durable checkpoint encoding, replay manifests,
host persistence, and memory tools remain subsequent local-working-memory increments.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-context
```
