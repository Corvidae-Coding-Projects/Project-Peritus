# I4 completion: decision views and durable dependencies

Status: complete for I4, phase five. **I0–I4 are complete; I5–I6 remain incomplete.**
Architecture verdict: ready within the qualified scope below. Select
`local_context.decision_view = "conservative"` to use the compiler. `"history"` remains the
default and explicit rollback because the measured benefit depends on the workload.

## Completed behavior

The compiler derives a view from committed observations, exact source cards, literal governing
input, current host policy/state, pending outcomes, working dependencies and the current native
tool exchange. Only proven identical successful inspections coalesce. Errors, effects and
distinct source versions retain their original identities. Exact omitted bytes remain available
through scoped `context_sources` descriptors and the named retrieval tool. Partial JSON cards
are explicitly partial, with original byte offsets; they are never presented as complete files.

Model-authored dependency narrowing is not trusted. The host binds each proposal to the actual
published and admitted originating request, including its full fingerprint, current literal
input, the managed candidate and known supplied files. Direct-folder namespace identity,
execution/external inputs, ignored or unproven file coverage, directory/search metadata,
pending effects and missing or changed request bindings require
recheck. Such structurally valid proposals remain retained as explicitly stale entries.
Existing unproven conclusions become stale before being used as current conclusions. Staleness
is sticky across restoration of old bytes, restart and selection-policy rollback.

The full tool catalog, argument/result admission, permission and opaque/protected-path rules,
role namespaces, current candidate observations, independent review and acceptance gates remain.
Reconstruction keys are private identities, not reuse grants or verdicts. No additional model,
remote memory service, compaction inference or lifetime work quota is introduced. Both completed
batch and request checkpoints, full artifact verification, fsync and publication CAS remain.

Immutable source classification now advances with the derived suffix index. Dependency capture
does not repeatedly reopen every archived result to classify its input type. This derived bit
never substitutes for fresh candidate/file observation, current admission or permission checks.
No local latency improvement is attributed to that change without a separate measurement.

## Regressions found and fixed

| Saved reproduction | Original failure | Final behavior |
| --- | --- | --- |
| `i4-host-before-r2` | A newly assembled but unbound request inherited the preceding request's dependency binding | Public assembly starts an unbound epoch; internal batch/guard checkpoints preserve the active response's origin |
| `i4-native-closure-before/native-closure.json` | Trailing host feedback displaced the latest complete tool exchange; the compiled view contained no native call/result pair | The compiler retains the most recent tool-containing exchange and every following message in original order |
| `i4-resolved-progress-before` | Direct, changed-managed and migrated completed plans became stale and appeared unfinished | Declared investigation status is retained separately; stale resolved plans remain completed progress and are not selected as unfinished work |
| `i4-ignored-input-before` | An ignored, undeclared successful read was treated as managed; changing it left its conclusion open | Current Git ignore coverage is checked before binding; excluded or unproven reads require recheck, while ordinary untracked source remains candidate-bound |

The native-exchange defect was also observed through actual Codex inference: both exact file
facts remained in the request, but repeated reads received visibility receipts and the run
continued requesting the same file. The saved first run has 21 completed host calls before
controlled termination of its owned process tree. A preliminary prose-order hypothesis was
disproved by the saved view, which contained no continuation notes; that experiment was removed.
The concrete grouping defect has a small public-interface replay with trailing host feedback.
The final unchanged two-file task completes and continues after a fresh process and source edit.
Stopped/incomplete native attempts are retained as failures with unobserved cost, not zero-cost
or passing performance samples.

## Explicit formats, migration and rollback

The final minimum reader for new host dependency semantics is **checkpoint schema 6**. This
also fences out the earlier schema-5 prototype, whose bindings and declared-progress semantics
were not qualified. Schema 6 is committed before a new host event. Failed publication prevents
that event. The reader still accepts application checkpoint versions 1–5; v5 state is decoded,
then conservatively migrated before new semantics are written. Unproven active conclusions
become stale, with original declared progress retained where known. Unknown legacy status is
never invented. Existing v4 plan evidence survives advancement to v6.

| Contract | Final encoding |
| --- | --- |
| Ordinary C6 events, unbound snapshots/pages | Existing version-2 bytes; earlier supported snapshots/events remain readable |
| Host delta/invalidation events | Working codec version 3, tags 4/5, with ordinary proposal fields and host constraints |
| Bound entries | Working snapshot/entry-page version 4; original request/source binding plus optional declared investigation status |
| Observation, header and protocol pages | Existing version-2 bytes; sealed observation/source reuse remains valid |
| Host transcript semantics | Version 2 under checkpoint v6; version-1 prototype state remains readable for migration |
| Working-entry pagination | New `entry-v2` prefix under v6; old byte cursors reject instead of addressing changed metadata |

Version-3 prototype bound entries remain readable. The decoder infers only status that is
actually known; earlier stale/superseded records can have unknown declared status. Current
and declared statuses, bindings, graphs, versions and canonical fields are validated. Version-4
entry data is rejected under a checkpoint below v6. Model updates cannot supply host bindings,
assign host-derived status or bypass the ordinary source/graph checks.

Setting `decision_view = "history"` rebuilds from the latest committed state using the current
reader. It keeps v6, exact sources, pending outcomes, host dependencies and completed progress.
It does not restore an old state directory or permit an old binary to skip new events. The
preserved phase-four reader explicitly refuses v6 before replaying its tail or admitting work.
Ordinary runs remain at their previous reader floor until the new semantics are needed.

## Final real-interface qualification

All mutable evidence is fixture-owned under
`/home/doll/.cache/peritus-efficiency-dif0njrm/candidate/`. No live user workspace or daemon was
used as test state. Drivers exercise production public APIs, actual disk/journal/artifact stores,
actual host tools and official providers. No provider replies, tool observations, gate results
or usage values are substituted.

| Final campaign | Observed evidence |
| --- | --- |
| `i4-completion-compiler` | Eight groups: exact facts/current host state, undeclared same-size/restored-mtime and mode changes, complete bounded descriptor paging, source restrictions, reviewer isolation, rollback, partial raw JSON continuation and complete native closure after host feedback |
| `i4-completion-ignored` | Two real read/update/restart cases: an ignored undeclared dependency requires recheck; ordinary untracked source stays usable and invalidates after a same-size/restored-mtime edit |
| `i4-completion-host` | Fifteen groups: exact/missing/altered/stale request bindings, unknown/error/external inputs, direct-folder uncertainty, legacy migration, v4 plan retention, graph/contradiction retention and atomic rejection, publication failure and three actual debugger-controlled kills around migration |
| `i4-completion-resolved` | Three completed-plan cases retain `declared_status=resolved` while effective status is stale; they stay out of unfinished-work selection and survive restart |
| `i4-completion-interruption` | Two actual kills immediately before/after host delta commit recover absence/presence respectively, without provider submission, tool redispatch or invented completion |
| `i4-completion-compatibility` | Actual phase-four v5 writer creates state; current reader opens it, commits v6, preserves resolved progress, rejects the old cursor, refuses the old reader and retains history rollback |
| `i4-completion-plan` | Eight existing sequential-plan/source/restriction/continuation groups under the compiler; execution remains opt-in |
| `i4-completion-varied` | Nine groups/264 public interface actions, including exact Unicode/CRLF pages, EOF/streaming, eviction, same-size/mtime/inode/mode changes, current authority, pending ordering and invocation topology |
| `i4-completion-input` | Current literal input invalidates the old conclusion; exact stale retrieval, sticky invalidation, unchanged source pages and fresh-process recovery pass |

Published roots are independently enumerated from committed SQLite state history and matched to
their publishing transactions. Every manifest/component/page has its declared size and SHA-256,
and the complete dependency set agrees. Finalized but unpublished manifests left by a failed
publication are distinguished from committed roots. An early checker incorrectly treated those
orphan artifacts as published; the retained final checker uses committed history instead.

The direct consumer's fixture profile was raised from 32 to 64 tool slots so the full catalog
plus optional memory/plan definitions can be bound. Production provider limits are unchanged.
The initially too-small pagination fixture correctly rejected its metadata envelope; the
compatibility replay uses a larger bounded page and verifies the resulting real cursor.

## Actual continuation, review and acceptance

Final Codex `gpt-6.1-sol` continuation (`i4-completion-continuation`) uses the same lineage
across two native processes and literal input revisions. The first process returns both exact
facts in three requests; the second follows a same-size/restored-mtime Alpha edit, reads only
the changed file and returns the new Alpha plus unchanged Beta in two requests. Input is
46,924 and 32,518 tokens respectively, with 15,872 and 7,936 cached tokens. All source bytes
match independent expectations. These are correctness journeys, not a speed comparison.

Actual independent review (`i4-completion-review`) follows writer-only scratch committed
through C6. The reviewer reads the real file in its own namespace and returns the exact expected
value. The published reviewer view contains the source and excludes writer scratch; both
namespaces remain on disk. Two requests use 31,251 input, zero cached input and 201 output
tokens. Both request/checkpoint bindings agree.

The managed CSV journey (`i4-completion-managed`) reaches **Accepted** through real design,
writing, the existing CSV gate and typed independent review. Only the intended file changes;
its bytes, original Git HEAD/index and project manifest are independently checked. Seven
requests use 143,513 input, 9,984 cached input and 990 output tokens. Current candidate capture
uses the original `separate` policy; this does not qualify or enable I6. This case does not
exercise a nonempty live review finding/fixer cycle; local contradiction/graph conservation and
literal current-state retention are covered separately, and review/fixer admission code is unchanged.

## Final-build paired cost observations

The final build uses official OpenCode 1.19.0/Zen with selected model `gpt-6.1-sol` in
`i4-completion-inventory` and `i4-completion-chain`. The command omitted `--model` and therefore
used the driver's default instead of the earlier DeepSeek selection. This is a separate model
cohort; its samples are not pooled with DeepSeek. Replay instructions now show the explicit
selection. All six cases complete, retain exact independent outcomes and unchanged source bytes,
and reconcile every normalized usage partition to canonical C5. Unknown billing stays unknown.

The two inventory pairs alternate order. Every case performs all eight ordered list/read pairs,
with the same 17 logical requests, 17 adapter inferences and no format correction. The directory
contains the same 96 other entries. Totals across both pairs are:

| Final-build inventory | History | Conservative |
| --- | ---: | ---: |
| Required host calls | 32 | 32 |
| Logical requests / adapter inferences | 34 / 34 | 34 / 34 |
| Format corrections | 0 | 0 |
| Total input | 399,008 | 373,026 |
| Cached input subset | 0 | 0 |
| Output including reasoning | 1,476 | 1,404 |
| Reasoning subset | 116 | 50 |
| Native total tokens | 400,484 | 374,430 |
| Completion median, seconds | 61.745 | 59.867 |

Complete input falls **6.5%** with equal requests and required operations; individual pairs save
6.53% and 6.49%. The observed completion median is 3.0% lower. The first case overlapped a final
correctness journey, and native execution varies; this is not an isolated latency-distribution
or universal speedup result. No cache-hit increase is claimed.

The final simpler eight-file chain has equal nine-request/eight-read counts and no corrections:
input is 72,688 versus 70,892 (**2.5%** lower), output is 498 for each, and total input/output is
73,186 versus 71,390. Elapsed time is 31.111 versus 32.258 seconds, so this control does not show
faster completion. The compiler remains opt-in and History stays the default. Raw observations,
commands, consumer identity and aggregate values are retained in `i4-completion-comparison-*.json`.

## Earlier DeepSeek cost cohort

These retained paired measurements use the preceding compiler cohort, before the final
ignored-file coverage guard (`i4-qualified-inventory-final`, `i4-qualified-chain-final`). They
remain separate from the final-build model cohort above and are not substituted for its
qualification. OpenCode 1.19.0/Zen, `deepseek-v4.1-flash`, uses the phase-four once-only schema layout in both
selections. Full input includes cached input. Each final response is independently decoded from
canonical C5 and reconciled to cumulative accounting. Format corrections and extra requests
remain included. Provider-cost and other absent counters stay explicitly unknown.

The repeated-inventory workload requires eight actual ordered list/read pairs through a linked
JSON chain, with 96 other directory entries. All exact independent values are returned, every
required host operation occurs and every file byte is unchanged. Two pairs alternate order:

| Combined two pairs | History | Conservative |
| --- | ---: | ---: |
| Required host calls | 32 | 32 |
| Logical model requests | 34 | 26 |
| Observed adapter inferences | 35 | 27 |
| Format corrections | 1 | 1 |
| Total input | 448,783 | 327,560 |
| Cached input subset | 47,104 | 91,392 |
| Output including reasoning | 12,190 | 11,341 |
| Reasoning subset | 10,042 | 9,527 |
| Native total tokens | 460,973 | 338,901 |
| Completion median, seconds | 60.585 | 49.824 |

Observed complete input falls 27.0%. One compiler run batches the same 16 host operations into
nine requests instead of 17; this is included in the outcome and prevents attributing the full
reduction solely to serialization. In the equal-request first pair, both use 17 requests/18
adapter inferences/16 host calls: input is 230,952 versus 216,105, a **6.4%** reduction. Natural
reasoning, batching, corrections and vendor caches vary. Some final correctness validation ran
concurrently; the timing values are observations, not an isolated or universal speedup claim.

The simpler eight-file chain (`i4-qualified-chain-final`) has equal nine-request/eight-read
counts and no correction: input is 77,656 versus 77,233 (0.54% lower), output 935 versus 1,174,
and elapsed 23.879 versus 24.124 seconds. Total input/output is 78,591 versus 78,407. This workload
does not show faster completion. The history default is retained rather than claiming the
compiler benefits every task. Earlier unqualified prototype timings and the stopped read loops remain in
the evidence root and are excluded from successful paired measurements.

## Reproduction and source coverage

Build/check commands all pass:

```sh
cargo build --locked --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
cargo clippy --locked --jobs 2 -p peritus-context -p peritus-agent \
  -p peritus-product-runner -p peritus-daemon --all-targets --all-features -- -D warnings
cargo check --locked --workspace --all-targets --all-features --jobs 2
cargo fmt --all --check
git diff --check
```

Final-build logs are `i4-coverage-guard-build.jsonl/.log`, `i4-coverage-guard-clippy.log`,
`i4-coverage-guard-workspace-check.log`, `i4-completion-fmt.log`, `i4-completion-diff-check.log`
and `i4-completion-link.log`. `i4-completion-local-receipt.json` records all nine replay commands.
Earlier `i4-qualified-*` runs remain retained separately; the completion cohort includes the
ignored-file coverage guard. The qualified consumer hashes are:

| Artifact | SHA-256 |
| --- | --- |
| Public context/tool consumer | `c48ee471573816191d92ac123ad8975a6629f85dec0f0eba5823df05e79dd8a3` |
| ProductRunner consumer | `c9ef21ba185bd045cad6cbc055c176aa942d05b9c2ff8ff6ec85fd4c38120f8c` |

The focused source identity inventory is `i4-completion-source-identity.json`. It binds dirty
source and manifests; the Git HEAD alone does not identify uncommitted work. Coverage traces
C6 event/delta/invalidation/entry codecs and page reconstruction; host environment/source
admission; request assembly, exact admission binding, migration and checkpoint publication;
descriptor/raw-byte paging; compiler ranking/classification/native closure; role isolation;
ProductRunner/DeveloperLoop callers; C5 trace/accounting and current candidate/review boundaries.
This is an I4 coverage record, not a whole-codebase audit or hash-inventory-as-coverage claim.

See [the reproducer instructions](reproducers/incremental-context/README.md). Source/data seeds
are fixed, including 3718225 for linked-file comparisons. Fresh runs consume real provider
inference; `--resume` rechecks completed cases with matching binaries without resubmitting
partial or unqualified attempts. Other runtime backends, Windows, models beyond these cohorts, UI journeys and a live
nonempty finding/fixer cycle were not requalified here. Their absence is not a passing sample.
Earlier supported reader paths remain in code; this phase directly exercises v3/v4/v5 inputs
and v6 output rather than regenerating every historical v1/v2 fixture.

Branch `develop` and HEAD `d58ccbed426fba0a361a6533bb4ddb80726ba2ce` remain unchanged. Unrelated
work is preserved. No project commit, push, user service/configuration change or deployment
was performed. This completes I4, not the remaining I5–I6 or whole-design objective.
