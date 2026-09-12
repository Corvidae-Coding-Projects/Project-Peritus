# Accepted failure replay manifests

Each JSON file is a schema-versioned replay record for one accepted product defect. It binds the
finding to the original revision, a minimized deterministic schedule, the required boundary hit,
the logical failure signature, three fresh-fixture reproductions, the owning regression, and the
first fixing revision. At least three fresh fixtures are required; a manifest may record more.
Paths are relative to the repository root and commands run from that root.

Schema version 1 requires `id`, `classification`, `invariant`, `original`, `reproducer`, `fix`,
`fixed_replay`, `negative_control`, `cleanup`, and `limitations`. The `original` object records the
source revision and observed failure; `reproducer` records the exact test and reachability; and
`fixed_replay` records the expected post-fix result. These manifests summarize checked-in tests and
evidence documents; they do not replace the owning regressions.
