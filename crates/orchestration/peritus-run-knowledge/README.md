# peritus-run-knowledge

`peritus-run-knowledge` is Peritus's pure, verified model for retaining grounded knowledge across
writer, reviewer, and fixer rounds. It binds every section to an exact workspace candidate, source
digests, conversation revision, role, and creation sequence.

The crate decides which prior sections remain reusable, which become stale, and whether a role
packet must carry a changed fact or may carry a reference to current knowledge. Model-authored and
compacted summaries remain navigation-only and cannot satisfy authoritative evidence requirements.

It performs no filesystem reads, hashing, model calls, persistence, or prompt rendering. Callers
supply observed digests and effectful `peritus-context` code binds selected sections to exact
context nodes.

## Focused checks

From the repository root:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --package peritus-run-knowledge
cargo verus verify --package peritus-run-knowledge --locked --check-toolchain \
  --fwd-verus-args-to roots -- --no-cheating --rlimit 20
```
