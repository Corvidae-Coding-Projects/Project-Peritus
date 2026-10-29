# I1 completion: indexed context projections

Status: complete for I1, the second execution-efficiency increment. I0 and I1 are complete;
I2–I6 remain incomplete. The report covers the named projection paths, not the whole design.

## Implemented behavior

I1 reuses the existing checked constant-time observation lookup for candidate ranking instead
of repeating linear searches inside the comparator. Tool identities and latest tool sequences
use rebuildable maps. Fact and inspection
indexing start at the unprocessed suffix. Candidate ranking evaluates each cached key once,
and archive-byte accounting uses a checked running total. Ordering and membership of the
resulting provider view remain unchanged. Persistent history remains the source of truth.

Recovery validates the committed archive and reconstructs these projections before continuing.
No source-index cache is persisted, no checkpoint/codec version is added by I1, and no provider,
permission, role, tool, publication boundary or integrity check is removed. Other experimental
increments in this checkout retain their existing defaults and qualification status.

The private host setting `PERITUS_CONTEXT_SOURCE_INDEX=reference` selects the former linear
ranking lookups, tool-sequence discovery, full fact/inspection filtering, repeated rank-key
evaluation and archive summation. It retains the preexisting constant-time direct lookup and
the already indexed policy-prefix path.
The selection is captured when the memory owner opens; unsetting it selects indexed execution. Both paths
use the same state, authority checks, view builders and persistence. The reference selection
provides independent same-build comparison and rollback from current committed history.

Content-free counters measure actual examined archive records, suffix records, tool-identity
records, sequence-search records, archive-accounting records and rank-key evaluations. Counters
are optional diagnostics, never authority. They count the named paths, excluding tree-key
comparisons, index maintenance/rebuilds, mandatory checkpoint verification and unrelated work.
Zero examined records for a map miss or cached total does not mean zero CPU or filesystem work.

## Real-operation qualification

`indexed-projections.py` operates the production context port, real workspace tools, disk-backed
C0/C6 state and normal checkpoint publication. Two writer pairs alternate reference/index order;
each starts with 320 reads and reaches 671 retained source records, crossing the 512-record page
boundary. A third pair uses the separate reviewer lineage. The six cohorts execute 318 public
API actions and independently check 3,096 delivered source pages against fixture-owned bytes
and SHA-256 identities. Every returned answer, ordered view and rejection matches across modes.

The cases include malformed/foreign/zero/missing/overflow handles, invalid cursor/schema bounds,
failed tools, same-size/restored-mtime changes, live read revocation, opaque-reference exclusion,
pending proposal/actual settlement, fresh-process recovery and a subsequent invocation. There
are 11 explicit rejection checks per cohort. All source files end with exactly the independently
expected bytes. Source-index cache hits never override current admission or file freshness.

Opening the public context port starts a new invocation. Recovery therefore checks exact
retained source retrieval and absence of redispatched old calls, then compares reference and
indexed views for that new invocation. It does not incorrectly require the old invocation's
native message list to be replayed. Leave-alone constraints govern writes; the read-restriction
case uses the actual opaque-reference policy. Early harness failures from those two assumptions
are retained separately and were corrected against the existing public API contract.

The retained varied replay also passes all nine case groups and 264 actions, including Unicode
and byte continuation, document eviction, filesystem changes, current authority, current host
policy and pending exchange ordering. Eight checkpoint integrity cases pass: exact roots, fresh
recovery, disabled page reuse, the preserved older binary, and missing/truncated/replaced/
same-size-corrupt pages. Failed recoveries preserve the committed root.

`source-index-trace.py` adds six cases using the actual native run's C7 observations. Both modes
reopen and deduplicate the original tool identities with equal recovered views. Altered tool
outcomes and noncontiguous tool sequences reject without changing the committed root. Only
owned C7 copies are damaged; provider C5 bytes are left unchanged.

## Measured local work

Both large-history pairs produce these identical structural counts per cohort:

| Named operation | Reference | Indexed |
| --- | ---: | ---: |
| Archive/ranking records examined | 46,736,629 | 155,324 |
| Suffix records examined | 132,357 | 6,280 |
| Tool-identity records examined | 107,440 | 0 |
| Latest-tool sequence records examined | 2,658 | 0 |
| Archive-accounting records examined | 19,655 | 0 |
| Candidate rank-key evaluations | 151,876 | 9,276 |

Named archive/ranking visits decrease 99.668%; rank evaluations decrease 93.892%.
The preexisting direct lookup still examines at most one record in either mode. I1 reuses that
path for ranking. Counts include the original process and its fresh recovery owner, with
counter resets accounted for explicitly.

Six warm public assemble-plus-checkpoint calls are sampled per cohort. Their elapsed times
include publication and mandatory integrity work, not just the in-memory selector:

| Workload/order | Reference median ms (range) | Indexed median ms (range) |
| --- | ---: | ---: |
| Large writer, reference first | 102.474 (99.667–103.094) | 68.816 (67.864–83.365) |
| Large writer, indexed first | 191.011 (186.384–203.717) | 174.264 (170.953–181.753) |
| Small reviewer | 106.996 (102.508–110.022) | 103.669 (99.295–112.459) |

The two large writer pairs improve about 33% and 9%; the small reviewer pair improves about
3%. The absolute timings vary substantially between pairs. Earlier exploratory data included
a small-reviewer regression around 5%. These observations support reduced local work, not a
universal latency improvement. OS caches are warm and diagnostics are enabled. No build or
other owned campaign overlaps these final paired samples.

The original exploratory reference mistakenly made every archive lookup linear. Those stress
runs remain retained but are excluded from this baseline. The final reference preserves the
already efficient direct lookup and policy prefix, and restores only the actual former ranking,
fact/inspection, tool-identity/sequence and accounting work. Full tool arguments and results
are included in the ordered view comparison. Warm diagnostic windows are selected after the
seeding batches, including their separate completion checkpoints; no timings are manufactured.

## Actual provider continuation

Two actual Codex `gpt-6.1-sol` chat runs use the same source bytes, task, profile and workspace
with fresh private state, once per index selection. Both return the exact independent JSON
answer, read both real files, preserve all bytes, and have identical initial canonical request
hashes. Each makes two provider requests. Indexed reports 30,713 input /
0 cached input / 279 output tokens; reference reports 30,721 input /
7,936 cached input / 194 output tokens. Canonical C5 high-water accounting independently
reconciles each run. Missing optional counters remain unknown. Vendor cache acceptance and
model-generated continuation differences are separate from local projection reuse.

Elapsed time is 18.734 seconds indexed and 15.318 seconds reference. These single native runs
verify unchanged functionality; their variable output/cache behavior does not establish a
provider-latency improvement. I1 preserves request semantics rather than claiming token savings.
No fixture source text or workspace path occurs in any of the 10 checked diagnostic files.

## Evidence and rollback

Evidence root: `/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`.

- `i1-qualified-indexes/result.json`: three paired local campaigns plus actual provider runs.
- `i1-qualified-varied/result.json`: retained varied API campaign.
- `i1-qualified-integrity/result.json`: checkpoint integrity and older-reader campaign.
- `i1-qualified-trace-recovery/result.json`: real-trace identity replay and rejection cases.
- `i1-complete-build-r2.log`: pinned, locked daemon/CLI/TUI build with two Cargo jobs.
- `i1-complete-clippy.log`: affected all-target/all-feature strict Clippy, including daemon,
  product runner, agent and context owners.
- `i1-complete-fmt.log`: full workspace formatting check; `git diff --check` also passes.
- `i1-final-source-identity.json` and `i1-final-binaries.json`: exact source/build identities.
  The source hash inventory is provenance, not a claim of whole-codebase reading coverage.

The private reference setting reopens current history and retains all observations and command
ownership. Remove the setting to return to indexed execution. Linux/Codex is the live qualified
provider/platform combination; other providers/platforms and later-stage campaigns remain
explicitly unqualified. No repository commit, push or deployment is part of this completion.

Architecture verdict: **ready for I1**. Its completion does not qualify the later increments.
