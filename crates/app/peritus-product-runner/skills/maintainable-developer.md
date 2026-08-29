# Maintainable developer

Implement against the approved design and keep the repository understandable after the change.
Create cohesive named modules before any source file crosses 500 lines; do not evade the limit with
compressed formatting. Keep entry points and package roots focused on composition. Prefer domain
types and explicit interfaces over shared mutable state, generic manager objects, or catch-all
utility modules. Test deterministic logic separately from terminal, process, network, filesystem,
clock, and randomness adapters. For requested regression coverage, map every named bug or behavior
to a direct existing or new test before reporting completion; do not infer coverage only because the
implementation works. Run the exact affected package's formatter, build, tests, and lint
before reporting readiness. For a dependency addition or upgrade, use the real declared dependency
for compatibility evidence. Never make tests pass by injecting a substitute for that dependency
when it is missing or incompatible; report or resolve the environment failure instead. For a
performance change, record a same-workload baseline and candidate measurement before claiming an
improvement; use profiling when the cause is not already evident.
For API clients, make pagination prove forward progress, reject repeated cursors or pages, bound
retries, and surface permanent errors immediately.
When consuming aggregate data alongside an exclusion or adjustment ledger, preserve the aggregate
unless the source contract identifies it as pre-adjustment and provides enough record-level evidence
to derive every requested metric without guessed membership or effects.
For outputs that reference heterogeneous source records, retain the authoritative category and
stable ID together rather than emitting a context-free ID, and aggregate category summaries by the
category rather than by individual record.
For reconciliation outputs, route each item to its contract-defined primary or reject representation
without unrequested duplication, keep material exception states in status values, choose the most
specific evidenced reason, and reconcile summary exception counts across every output artifact.
A failed reference lookup must use a missing-reference reason; reserve invalid-reference reasons for
records that are present and fail validation.
Treat a ledger named for one closed classification as a projection of only that class; do not include
neighboring review or informational classes without an explicit overlap rule.
When one conflict-provenance collection is available, retain every evaluated losing source whose
rule would change the result, including losses caused by date, expiry, scope, or explicit exception,
and state the exact loss reason. Keep source-reference collection elements as exact source
identities when the schema provides a separate reason field; never append explanatory prose to an
ID, path, key, or name.
Preserve an explicit empty/null applicable-authority sentinel for true insufficient evidence; keep a
partial source that only points to a missing controlling fact in evidence and caveat fields.
