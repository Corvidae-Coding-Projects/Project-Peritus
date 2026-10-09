# FINDING-0006: Filesystem traversal accumulated all unsupported diagnostics in memory

Severity: medium

Blocking: true

Disposition: fixed in the final reviewed candidate.

Even after diagnostic cursoring, the tool traversal collected every projected omission into a global vector and sorted it after the walk. A workspace with many unsupported children therefore retained memory proportional to the entire omission set despite paged output. The repair emits deterministic walk diagnostics directly into the bounded omission page/digest accumulator.
