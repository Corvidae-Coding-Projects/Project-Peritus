# Diagnostic benchmark failure report

Snapshot date: 2026-09-02

Repository branch: `feature/g4-conversational-runs`

Repository HEAD at review: `300d6e015cfd6326bdc601d30c922810ca393c3e`

Status: diagnostic evidence, not fixed-build release qualification

## Purpose

This report organizes every retained HarnessBench and Terminal-Bench failure without deleting,
rerolling, relabeling, or score-adjusting inconvenient results. It separates four facts that must
not be conflated:

1. What the benchmark officially observed.
2. Whether the observation exposed a Peritus defect, a model/candidate miss, an adapter or provider
   failure, or an evaluator/fixture defect.
3. Whether a general correction is now present in source.
4. Whether that correction has been independently requalified with one immutable Peritus build.

The complete task-level inventories are in
[the HarnessBench appendix](harnessbench-failure-inventory.md) and
[the Terminal-Bench appendix](terminalbench-failure-inventory.md). The
[Terminal-Bench adverse-trial ledger](terminalbench-adverse-trials.md) names every one of the 347
trials with at least one adverse dimension. The detailed causal evidence,
trial names, corrections, and integrity decisions remain in
[the failure journal](failure-journal.md).

## Accounting rules

- Official outcomes remain official. A source fix does not rewrite an earlier score.
- Reward-zero, unscored, timeout, native-rejection, provider-failure, and evaluator-defect records
  all remain visible.
- A verifier pass is not called a clean pass when Peritus itself rejected or failed the run.
- A Peritus acceptance is not called a task pass when the external verifier returned zero.
- Provider and benchmark defects are not silently charged to candidate quality, but they are not
  removed from the campaign totals either.
- A result affected by a contradictory or unpublished oracle rule remains recorded at its official
  score. Peritus will not learn the hidden literal merely to make the benchmark green.
- A method-invalid pass remains an official pass with a separate integrity warning; it is not used
  as clean evidence of the claimed capability.
- No post-hoc task-specific prompt, expected answer, verifier lookup, reroll selection, timeout
  inflation, or prebuilt artifact is an acceptable correction.
- `uncorrected` means either no general source correction exists or the correction has not been
  closed by a complete fixed-build qualification. Those states are reported separately below.

## Evidence corpus and limits

### HarnessBench

The complete diagnostic aggregate is:

`/home/doll/.local/state/peritus/benchmarks/harnessbench/2026-08-28-baseline/reports/campaign-diagnostic-baseline-v1.json`

Its SHA-256 is `26a981ef968443504a0d47d420e34783fa99c2b9d8b1d876661415175a8dbd3e`.
It contains all 106 expected tasks from pinned HarnessBench revision
`1025086a446653702b80cfb48babbeec35db6b2c`, selected with
`latest_result_per_task_by_mtime_then_path`.

The benchmark did use Peritus. Every selected record has a native invocation of
`peritus-benchmark-agent harnessbench`, a Peritus design/writer/check/reviewer trace, and provider
roles. The aggregate nevertheless used `allow_legacy` identity policy: 106 native invocations have
no source revision or binary SHA. Because the selected task results were produced while the harness
was evolving, this is a complete diagnostic aggregate, not one fixed-version campaign.

A later fixed-candidate attempt under
`/home/doll/.local/state/peritus/benchmarks/harnessbench/final-9c91ad4d14ff` produced results for
tasks 001 through 021 only. It has no complete campaign report. Earlier final-attempt directories
are also incomplete. These partial attempts must not replace the complete diagnostic aggregate or
be advertised as a final score.

### Terminal-Bench

The complete frozen aggregate is:

`/home/doll/.local/state/peritus/benchmarks/terminalbench/reports/frozen-baseline-445.final.json`

Its SHA-256 is `d7feff820c7d38d204744f75ef9214cb7b91949cac2c8c3b5625f10c39321bc0`.
It contains all 445 expected trials: 89 pinned Terminal-Bench 2.0 tasks at five attempts each,
using Harbor revision `61095f400a2f22673c43f672001580baa5e91480` and Terminal-Bench revision
`ab3575606830479be548b69e9961815e83c6f5e7`.

The benchmark did use Peritus through
`benchmarks.external.terminalbench.peritus_agent:PeritusAgent`. Native reports name the Peritus
writer and reviewer roles and the `peritus/gpt-5.6-sol-claude-sonnet` harness composition. The
aggregate records binary SHA
`ed0ef30eb5dda2817ebd8a02e46062b7c5a7400e22ee04653d5106d3e6ffb1e7`, but the legacy native
reports do not bind themselves to that binary or to a source revision. It is therefore a frozen
diagnostic baseline, not reproducible proof for the current repository HEAD.

## Honest top-line accounting

| Suite | Complete unit | Official result | Native/process result | Integrity qualification |
| --- | ---: | --- | --- | --- |
| HarnessBench | 106 tasks | Mean outcome 0.89685; 40 perfect and 66 partial | Mean process 0.92865; 12 perfect and 94 partial; security 1.0 on all 106 | Complete diagnostic aggregate across an evolving Peritus build |
| Terminal-Bench | 445 trials | 239 reward 1, 151 reward 0, 55 unscored; scored accuracy 0.61282; completed success rate 0.53708 | 134 native accepted, 245 native rejected, 66 missing native reports; 108 exception trials | Complete frozen campaign using legacy, non-source-bound invocation reports |

Only 98 of the 445 Terminal-Bench trials simultaneously had reward 1, native acceptance, and no
exception. The other 347 trials had at least one adverse signal. This does **not** mean all 347 had
bad artifacts: 141 reward-one trials were natively rejected and 24 reward-one trials carried an
exception. Conversely, 27 reward-zero trials were natively accepted. The disagreement is itself a
major harness signal and remains in the accounting.

## Open or incomplete Peritus remediation

These are the clearest product or workflow gaps that still need implementation, enforcement, or
fixed-build qualification. The table distinguishes partial source guidance from a mechanically
closed capability.

| ID | Evidence | Uncorrected general gap | Required direction |
| --- | --- | --- | --- |
| HBE-001 | HarnessBench task 011 and cumulative campaign behavior | Each external debugging round can repeat design, writer, gates, review, and large source reads instead of reusing validated state. The worst retained run used 139 requests, about 2.39 million tokens, and 1,417.6 seconds. | Reusable design state, semantic conversation compaction, explicit round boundaries, and visible cache/reuse accounting. |
| TBF-037 | `query-optimize__hbtMdm7`, reward 0 | Peritus accepted a structurally plausible SQL optimization without comparative timing; the candidate was about 39 percent slower than the reference. Current developer guidance asks for a same-workload comparison, but acceptance does not yet require typed measurement evidence. | Make measured baseline/candidate comparison mandatory for performance claims, with cheap staged samples and bounded commands. |
| TBF-041 | `cancel-async-tasks__5rUhw9b`, reward 0 | Peritus accepted an internal cancellation simulation in place of the requested process-level lifecycle ingress. | Exercise named signals, restarts, timeouts, disconnects, or crashes at the real public boundary when the environment permits it. |
| TBM-001 | `kv-store-grpc__r9sYjKe`, reward 0 | The candidate mirrored response field `val` into the request schema instead of the explicit request field `value`; review did not finish. | Preserve as a legitimate model/candidate miss and test whether ordinary review/recovery catches it after the systemic reliability work. |
| TBM-002 | malformed-HTML sanitizer trials, reward 0 | A hand-written byte-preserving parser did not match browser error recovery. | Treat as a real implementation-capability miss; improve browser-equivalence validation rather than teaching expected fixture output. |

The last two entries may be model variance rather than missing Rust machinery, but they are not
closed and are not benchmark gotchas.

## General corrections present in source but not campaign-closed

The journal records general corrections for the following product, adapter, and workflow findings.
Focused tests or selected unchanged reruns often passed, but there is no complete fixed-build
campaign proving that the current source closes every recurrence. They therefore remain open at
the qualification layer:

- Harness product findings: `HBF-001` through `HBF-030`, plus `HBA-001`, `HBS-001`, `HBC-001`,
  `HBM-001`, `HFC-001`, and `HBT-001`.
- Harness integration findings: `HBI-001`, `HBI-004`, and `HBI-027`.
- Harness later-task generalizations: `HBI-048` through `HBI-052`, `HBI-055` through `HBI-059`,
  `HBI-061`, `HBI-062`, and `HBI-065`.
- Terminal infrastructure findings: `TBI-001` through `TBI-003` and `TBI-006`.
- Terminal product findings: `TBF-001` through `TBF-036`, `TBF-038` through `TBF-040`, and
  `TBF-042` through `TBF-057`.

This category includes real improvements such as provider failover, credential rotation,
caller-deadline propagation, bounded output and context compaction, artifact/repository
classification, exact-path semantics, role grounding, process-tree cleanup, requirement-to-test
mapping, opaque-interface enforcement, and identity-bearing reports. The frozen scores remain
unchanged. A new campaign is the closure mechanism.

Important residuals within this category are:

- `HBC-001`, `HFC-001`, `HBF-004`, and `HBF-008` reached correct or improved artifacts but remained
  request- and token-heavy. Task 037 still used 34 responses, 735,818 tokens, and 794 seconds.
- `HBI-055` through `HBI-059`, `HBI-061`, and `HBI-065` improved broad behavior but retained lower
  official outcomes due to remaining private lexical/schema rules or generative variance.
- `TBF-006` needs an unchanged run to prove exact provider-terminal propagation at the adapter
  boundary.
- `TBF-009` demonstrated one successful recovery, but the frozen campaign still contains 175
  `no tool calls or usable final response` native failures.
- `TBF-032` corrected credential rotation, but a fresh authenticated request that crosses an actual
  credential expiry/rotation boundary is still required.
- `TBF-043` has current bounded-observation and compaction corrections, but the frozen campaign
  still contains context-window failures and has no final-candidate rerun.
- `TBF-047` gained daemon-owned command handles for poll, stdin, resize, signal, cancellation, and
  recovery in `e9da73a0`. The frozen failure predates that work; the complete active-command path
  still needs fixed-build external qualification.
- `TBF-057` added a run-scoped provider circuit after the frozen report. No external campaign has
  measured it.

## Retained HarnessBench evaluator and contract failures

The following findings remain external to Peritus's published task contract. They are not removed
from the official scores, and Peritus has not been taught their hidden strings or schemas:

- `HBI-002`, `HBI-003`: impossible quality threshold and ground-truth lookup outside the task.
- `HBI-005` through `HBI-026`: hidden taxonomies, invalid slots, unmatchable terms, exact-prose
  tests, unpublished labels or schemas, contradictory citations, negation-blind matching,
  incomplete synonym sets, path/glob mismatches, and raw documentation/test/release-note tokens.
- `HBI-028` through `HBI-047`: contradictory inventory boundaries; unpublished state, severity,
  status, compensation, ledger, reply-key, and serialization conventions; missing script inputs;
  negation-blind checks; contradictory grading; and redundant network requirements.
- `HBI-053`, `HBI-054`: unpublished severity/reject/summary rules and cross-session campaign plus
  duplicate-bot-routing assumptions.
- `HBI-063`: a refusal vocabulary that excludes the ordinary supported phrase “does not specify.”
- `HBI-066`, `HBI-067`: unpublished partial-snapshot, ledger-shape, and pending-action alias rules.

`HBI-060` and `HBI-064` are clean controls, not failures. `HBI-001`, `HBI-004`, and `HBI-027`
were real integration issues and are listed in the source-corrected category rather than being
misclassified as benchmark defects.

## Retained Terminal-Bench evaluator and infrastructure failures

| ID | Retained issue | Honest treatment |
| --- | --- | --- |
| TBI-004 | `install-windows-3-11` verifier requires unpublished `/tmp/qemu-monitor.sock`; all five attempts scored zero despite programmatic interfaces. | Keep all five zeros; do not add the hidden socket path. |
| TBI-005 | PyTorch recovery verifier assumes a hidden forward-call signature that cannot be recovered from a state dictionary. | Keep affected zeros; do not infer private call syntax. |
| TBI-007 | Cold Torch/CUDA verifier downloads consumed seven verifier deadlines, with an additional network-sensitive variant. | Keep timeout/unscored records; distinguish verifier infrastructure from candidate quality. |
| TBI-008 | HTML verifier's byte-preservation expectations conflict with browser parsing, and one leaked Chromium/PID failure was treated as safe. | Keep the records and separately retain `TBM-002` as the real parser limitation. |
| TBI-009 | SAM `output_path` is required as both a directory and a CSV file. | Keep the four zeros and one unscored trial; do not invent task-specific path handling. |
| TBI-010 | MTEB retrieval ranking depends on unpublished SciFact prompt conventions. | Keep the four zeros and one unscored trial; do not inject the private prompt. |
| TBI-011 | Circular-plasmid junction grading chooses one hidden decomposition although an equivalent junction is valid. | Keep the zeros; do not encode the hidden decomposition. |

The corrected `TBI-001` through `TBI-003` failures—container registry short-name resolution,
glibc compatibility, and missing companion executable—remain in historical trial evidence but now
have general adapter remedies. `TBI-006` fixed identity capture only for new reports; identity
cannot honestly be backfilled into the frozen campaign.

## Terminal-Bench native and exception failure families

The 245 native rejections break down exactly as follows:

| Native failure | Count |
| --- | ---: |
| Provider returned no tool calls or usable final response | 175 |
| Two unchanged fixer cycles with exact checks or blockers remaining | 18 |
| Selected provider could not inspect required image inputs | 14 |
| Context still exceeded 200,000 tokens after compaction | 21 |
| Artifact workspace exceeded bounded design inventory | 6 |
| Anthropic authentication terminal | 3 |
| Managed worktree HEAD changed during the run | 2 |
| Missing compiler/toolchain escalated to the user | 2 |
| Other waiting-for-user handoffs: in-scope pre-existing failure and phantom path | 2 |
| Anthropic ambiguous acceptance | 1 |
| Provider subprocess timeout | 1 |
| **Total** | **245** |

The context count is the sum of the twenty-one distinct over-limit records, including repeated
919,141- and 1,343,398-token estimates. Those are not provider-capacity failures; they are product
context failures in the frozen build.

The 108 Harbor exception trials break down as:

| Exception family | Count | Detail |
| --- | ---: | --- |
| Agent timeout | 56 | 36 at 900 seconds, 7 at 1,200 seconds, 8 at 1,800 seconds, and 5 at 3,600 seconds |
| Runtime error | 45 | 29 stale/unsupported native schemas, 3 provider-router qualification failures, 3 missing `/app` workspaces, 9 missing developer-trace errors, and 1 process tree that survived cancellation |
| Verifier timeout | 7 | All at 900 seconds |

These categories overlap official rewards and native states. They must not be added to the 151
zeros as if they were disjoint. The task-level appendix preserves the overlap.

## Clean controls and positive evidence

Positive evidence is retained to calibrate the failure report, not to dilute it:

- HarnessBench had 40 perfect outcomes, 12 perfect process scores, and security score 1.0 on every
  task. Tasks 069, 099, and 103 are especially useful clean controls for authority, privacy, and
  state adaptation.
- Terminal-Bench had 239 official reward-one trials, but only 98 were also natively accepted with
  no exception. The narrower number is the honest clean-trial count.
- Several frozen reward-one artifacts survived native/provider/timeout failure. They demonstrate
  useful partial durability, while simultaneously demonstrating that completion and reporting are
  unreliable.

## Decision register

The following decisions remain for the project owner; none has been silently made in this report:

1. Prioritize the three systemic implementation or enforcement gaps: orchestration reuse
   (`HBE-001`), measured performance evidence (`TBF-037`), and real lifecycle-ingress testing
   (`TBF-041`). Qualify the already-landed interactive command lifecycle (`TBF-047`) with the same
   immutable build rather than reimplementing it.
2. Decide whether candidate-level misses `TBM-001` and `TBM-002` warrant new general workflow rules,
   stronger validation tools, or simply another honest sample after reliability fixes.
3. Decide which evaluator defects should be reported upstream. They should remain in Peritus's
   local score and report regardless of upstream response.
4. Require the next qualification campaign to fail closed when source revision and binary SHA are
   absent, rather than using `allow_legacy`.
5. Run one small authenticated expiry-crossing provider test before another costly campaign.
6. Only after the source gaps and identity requirement are complete, run a fixed-build campaign.
   Do not rerun merely to seek a luckier score.

## What would constitute benchmark cooking

The following actions are expressly rejected:

- Replacing the complete aggregate with only successful later task results.
- Dropping provider failures, missing reports, timeouts, or external-infrastructure failures from
  campaign totals without showing both raw and adjusted views.
- Teaching Peritus task IDs, verifier outputs, hidden exact strings, expected answers, private file
  paths, or benchmark-specific schemas.
- Looking at hidden model state or implementation after the task declares it opaque, even if the
  verifier awards a pass.
- Raising deadlines until a single benchmark passes while leaving general product scheduling
  unchanged.
- Selecting the best of repeated attempts as though it were the only attempt.
- Retrofitting source or binary identity into legacy records.
- Claiming that a focused regression or one unchanged task rerun qualifies the complete product.

Installing an ordinary missing compiler, Python, package manager dependency, or other declared
toolchain inside a disposable task environment is not cooking. It is legitimate harness capability
when done through a general, authorized prerequisite mechanism and when setup cost and failures
remain visible.

## Evidence map

- Detailed finding journal: `benchmarks/external/failure-journal.md`
- Harness task inventory: `benchmarks/external/harnessbench-failure-inventory.md`
- Terminal trial inventory: `benchmarks/external/terminalbench-failure-inventory.md`
- Terminal adverse-trial ledger: `benchmarks/external/terminalbench-adverse-trials.md`
- Terminal raw job log:
  `/home/doll/.local/state/peritus/benchmarks/terminalbench/jobs/peritus-terminalbench-2-k5-high/job.log`
- Harness complete aggregate: path and SHA above
- Terminal complete aggregate: path and SHA above
- Per-task workspaces, prompts, traces, native invocations, verifier outputs, and result JSON are
  referenced from those two aggregate reports and the failure journal.
