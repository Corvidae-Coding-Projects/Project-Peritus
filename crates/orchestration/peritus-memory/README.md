# peritus-memory

`peritus-memory` manages durable facts that agents may reuse in later turns. It records where each
fact came from, which workspace and role may see it, and whether the fact is still a candidate,
approved, quarantined, or deleted. Retrieval is deterministic, and its search indexes can be
rebuilt from the authoritative records.

The orchestration layer uses this crate when it assembles grounded context for an agent. Memory is
derived context, not project authority: it cannot replace repository contents, user instructions,
policy decisions, or fresh tool evidence. The crate also performs no ambient file or network I/O
and must not store secrets.

Lifecycle and retrieval logic live in separate modules so new ranking or retention policies can be
added without changing the durable record format.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-memory
```
