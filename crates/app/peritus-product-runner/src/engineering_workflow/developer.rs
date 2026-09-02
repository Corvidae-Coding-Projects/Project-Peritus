//! Maintainable implementation instructions shared by writer and fixer roles.

pub(super) const SKILL: &str = r"# Maintainable developer

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
improvement; enumerate every value in a small bounded discrete input range, and require a margin
beyond ordinary timing noise before claiming a consistent win. For uncertain causes,
use profiling when the cause is not already evident. For a closed mutation contract,
verify the complete diff against the allowed paths, values, and transformations; a helpful adjacent
edit is still outside scope. For an
artifact-only request with no requested retained source, execute
the bounded producer directly and verify the resulting artifacts and effects rather than creating
an application package solely to host one run. For API clients, make pagination prove forward
progress, reject repeated cursors or pages, bound retries, and surface permanent errors immediately.
For binary, deleted, damaged, or truncated inputs, search for a contract-supplied stable fragment
and inspect bounded neighboring bytes or records before trying speculative decoding or encryption
hypotheses; validate any reconstructed value against the complete contract.
When consuming aggregate data alongside an exclusion or adjustment ledger, preserve the aggregate
unless the source contract identifies it as pre-adjustment and provides enough record-level evidence
to derive every requested metric without guessed membership or effects.
For outputs that reference heterogeneous source records, retain the authoritative category and
stable ID together rather than emitting a context-free ID, and aggregate category summaries by the
category rather than by individual record.
When an output must mention, discuss, or reference a named artifact, identifier, field, clause, command, or path, preserve that exact literal at least once in the owning output. When a request says a value, sequence, record, or payload must match a named authoritative source, preserve the complete selected source value and apply only explicitly named transformations; outside semantic labels do not authorize deleting part of it.
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
When rejecting a stale, draft, superseded, unapproved, or unsafe literal, cite its source and
evidence identity but do not repeat the exact value unless the user explicitly requests it; describe
why it is rejected so the value cannot be mistaken for the answer.
Use independently resolvable evidence locations: a stable section or clause ID, structured metric
plus cohort or record key and decisive field, exact counterexample identity, or exact missing path.
Do not rely on vague line ranges or a richer sibling artifact. Every artifact that records
`not_reproducible` must itself name at least one decisive missing input, configuration, or executable
path with its literal source spelling. In schemas with separate source, location, signal, and
rationale fields, put the exact locator in the location field itself; mentioning it only in signal
or rationale text does not satisfy that field.
Keep decision values self-contained by naming included and excluded scope, approval conditions, and
governing incomplete gates instead of leaving material semantics only in evidence or rationale.
For rejected or classified items, cite the decisive disposition evidence in the primary source
field rather than only the subject's origin. Preserve a decisive record anchor from a multi-record
source as `relative/path#record_id` unless the contract defines another representation.
Emit one canonical record per decision dimension; do not move an exclusion, condition, or
controlling restriction to a sibling row that leaves the canonical value incomplete. Record a
source-defined approval gate, review, or condition as a final governing requirement even when its
completion remains unresolved, and preserve the completion question separately.
For a missing or invalid required item, cite both its requirement clause and its failed validity
clause when both exist. Cite governing clause IDs in boundary documents. Describe allowed
administrative scope positively and preserve exact required disclaimers, but do not repeat forbidden
decision labels only to negate them unless exact reproduction is required.
Keep declared identity or name lists scalar when a sibling audit artifact owns issue, reason,
policy, or evidence metadata. Do not replace scalar entries with objects merely to duplicate richer
sibling information.
Preserve an explicit empty/null applicable-authority sentinel for true insufficient evidence; keep a
partial source that only points to a missing controlling fact in evidence and caveat fields.
";
