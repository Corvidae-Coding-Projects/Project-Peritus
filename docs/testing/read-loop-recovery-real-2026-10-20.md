# Read-loop recovery: defects, repairs, and real testing

Baseline: develop `a392c61fcfa596cb97e1b88d79822a124a03b480` (v0.1.7).
This campaign addresses all twelve findings from the repeated-read investigation and additional
failures found while exercising their real paths. No unit tests, substitute providers, mock
suites, formal verification, or repository qualification suites were created or run.

## Observable contract

An unchanged observation already delivered in the current model view should be usable without
another full read. If omitted, its exact authorized archive can restore it. A relevant change,
new question, unavailable page, or current permission change can require fresh evidence. A retry,
unrelated command, no-op, or invocation rollover must not independently require rediscovery.
Current user instructions retain authority. Host feedback and prior assistant prose do not become
requirements, grant access, or establish an effect. Historical bytes are labeled historical and
never credited as current source grounding.

Expectations in the replayers come from fixture bytes, independently computed hashes, explicit
filesystem transitions, current permission settings, and role/task identity. A returned success
message alone is insufficient.

## Original findings and final disposition

Paths below are relative to `crates/app/peritus-product-runner/src/` unless stated otherwise.

| Finding | Root cause and repair | Behavioral evidence |
| --- | --- | --- |
| R01, P1: successful external reads disappear | `reference.rs` omitted a digest; assembly applied a workspace-only freshness rule. Explicit references now retain digest, current reference authority, private exclusions, and actual file identity throughout assembly and reuse. They cannot establish workspace grounding. | Original: six successful reads in two processes lost their bodies in the next view. Final: repeated external reads retain content; same-size replacement, permission revocation, cold restore, Unicode/space paths, and private exclusions are exercised. |
| R02, P1: retry/segment entry loses the next step | Invocation entry cleared recent messages and automatic selection omitted assistant progress. `assembly/continuation.rs` retains scoped assistant decisions, beginning and end of long next-step prose, and settled operation receipts as non-authoritative continuation. | Twelve rollovers and fresh-process reopening preserve the last next step; a long note retains both its decision and final action. Another role cannot inherit writer continuation. |
| R03, P1: mutation result/progress disagreement | Patch omitted `changed`, while its consumers disagreed about absence. Patch, write, and remove now return consistent settled outcomes; a no-op does not write or establish useful progress. | The actual provider performs patch, deliberate no-op, temporary write/read/remove, and final readback. The fixture checks exact physical bytes and the actual `changed=false` receipt. |
| R04, P1: correction explicitly orders rediscovery | Terminal-contract and premature-question corrections ordered new grounding. Corrections now include the rejected response and narrowly request contract repair. A reworded question/provider retry does not repeatedly reopen the same capability correction. | Source/caller review and an actual earlier live rejection of empty run instructions exercised repair without a new mandatory source inventory. Not every possible model question is dynamically generated. |
| R05, P1: reviewer repeatedly reconstructs immutable evidence | Coverage was invocation-local and prompted reconstruction each invocation. Delivery records now persist by logical role/task, source digest, length, and actual final-view ranges. Raw archival presence grants no credit. | Actual public evidence-reader pages reconstruct 6,939 UTF-8 bytes; final-view submission, rollover, cold reopening, role isolation, and changed digest are checked. |
| R06, P2: host warnings become requirement pins | Host feedback/screenshots were encoded as literal User input. A typed `HostObservation` context event preserves origin; current host observations are optional evidence rather than permanent user pins. | Under pressure, sixteen host notes do not become protected requirements. A literal user instruction with warning-like wording remains pinned. Existing old archives are not heuristically relabeled. |
| R07, P2: rotating read cycles evade tracking | A sixteen-entry exact-call set reset with invocations and missed cycles. A role-scoped metadata ledger survives rollover; already-visible results yield compact receipts and repeated-outcome feedback continues. | Original: 54 inspections across 18 targets yielded no warning. Final: 128 inspections across 32 directories yield feedback for all 96 repeats. |
| R08, P2: wrappers and refusals bypass tracking | Memory reads and early rejection paths bypassed the workspace observer. Outcome tracking now surrounds actual memory/base tools and observes inspection families and errors. | Sixty-four context reads, thirty-two skill reads, repeated missing/invalid/forbidden requests, then a valid changed-state recovery. Errors remain errors and establish no effects. |
| R09, P2: possible effects masquerade as progress | Possible command effects cleared inspection history; no-op patches counted as delivery; GUI movement broadly counted as coding progress. Digest-bound observations survive unrelated possible effects; explicit actual changes drive delivery. | Live no-op receipt and exact filesystem readback; continued repeat feedback across recovery. Stage-specific navigation remains available and is not credited as a coding edit. |
| R10, P2: local handoff needs unrelated permissions | Conversation handoff and, discovered later, candidate inspection both required all four capabilities. Local admission now requires Read/Write. Actual declared commands check current Process permission at launch; permission state participates in their report identity. | A real conversation → design → edit → independent review pipeline reaches Accepted with workspace Process and Network disabled. No model-issued command runs. Actual web/browser tools retain their separate permissions. |
| R11, P2: reuse/metrics omit major tool families | Measurement was inside MemoryTools and missed intercepted reads. The outer wrapper measures all inspection outcomes, repeated calls/bytes, and genuine first useful effects. Skills carry immutable resource identity; generic live queries still execute/freshly validate before byte-equivalent visible results can compact. | Context/skill repeats return real reuse receipts; workspace range and role controls remain intact. No blind HTTP or changing-command-output cache is introduced. |
| R12, P2: envelope fitting destroys typed document pages | Oversized file results became raw serialized JSON prefixes. Transport now fits typed UTF-8 source pages, exact range/cursor/digest and archive handle to the actual envelope. It loads each selected payload once during fitting. | Near-1-MiB ASCII, escaped Unicode, and multiline sources retain typed pages and valid continuation; generated overlapping ranges match independently generated line bytes. No arbitrary document-size cap is added. |

## Additional real defects found and repaired

1. **Normal relative aliases were rejected.** `./file` and repeated `././file` now normalize
   consistently for reads, scope enrollment, grounding, and private exclusions. Traversal, `.git`,
   symlink escapes, and forbidden absolute workspace paths stay rejected. The independent replay
   includes aliases combined with private and traversal paths.
2. **Writable in-place runs tried to enroll read-only external references as writable scope.**
   Absolute approved reference reads now use their existing reference authorization rather than
   workspace enrollment. Outside-root explicitly protected private references are also honored;
   this strengthens an earlier workspace-only exclusion contract.
3. **Archived source bytes bypassed current exclusions.** Ordinary views could reject a now-private
   file while `context_read` still exposed its original result, encoded ToolMessage, prior retrieval,
   or chained retrieval. `source_policy.rs` validates source provenance/current admission on direct,
   search, state-page, working-entry, and shortcut paths. Its bounded descriptor cache retains
   metadata, not wholesale trace bodies. The preserved independent reproducer fails on the original
   executable and passes for both local and external sources on the final executable. This is
   source-backed access control, not a promise to erase all mentions from unrelated prose.
4. **Useful failed-approach history disappeared after unrelated edits.** Historical caution remains
   distinguishable from current source evidence rather than silently disappearing.
5. **Later owned edits/deletions erased successful earlier reads.** A live accepted run wrote, read,
   and deleted a temporary file, then lacked its read body in the provider view and reported that
   it could not confirm the contents. Exchange assembly now retains those bytes explicitly as
   `before_later_settled_workspace_effect`, for same-batch and later-batch operations. It admits
   them under current source permissions, defers payload loading until selection, and excludes
   them from current grounding credit. Reopening actual completed writer state must show the
   original body/hash while the temporary file remains absent. External changes without a settled
   owned mutation still invalidate current observations normally.
6. **Transport fitting repeatedly loaded the same archived payload.** The selected near-1-MiB
   case read about 26.2 MB during the first typed-fitting implementation; after retaining the
   payload for the fit, measured input fell to about 3.23 MB. This removes avoidable reads;
   it is not a general latency benchmark or claim that typed fitting costs less CPU than the old
   semantically broken raw-prefix fallback.

## Final execution evidence

Final production libraries were built offline with the pinned toolchain and linked using Cargo's
exact emitted artifact cohort. The native consumer is an ordinary client of production
`WorkspaceDeveloperTools`, `MemoryTools`, `LocalContextHandle`, immutable developer-evidence APIs,
provider projection, journals, and ProductRunner. There is no test model implementation.

Final consumer SHA-256:
`75fc6a3a3623e1e39235e9f288a0abf23da4cd9db37d2f8298e2cb11f1756e2e`.
Original consumer SHA-256:
`a251e7e5ac4518129bb8bd6c7273923cbb292352f9a33babb9c8a41fe925c1a3`.

All final campaigns passed on that exact consumer:

| Campaign | Actual execution | Result / retained artifact |
| --- | --- | --- |
| Main regressions and generated ranges | 765 API actions; 12 scenarios; seed `20261020`, including 160 varied overlap/alias requests | PASS; `campaign-final20/results.json` |
| Immutable reviewer evidence | 48 API actions, two native processes, nine pages reconstructing 6,939 UTF-8 bytes | PASS; `review-evidence-final20/results.json` |
| Independent boundary/private-source oracle | 302 API actions across 38 native launches; seed `202610204719`; range reconstruction, same-size/restored-mtime changes, replacement, deletion/recreation, Read/chmod revocation, roles, aliases, original/wrapped/chained archive access | PASS; independent `final20-campaign.json` and checked-in `independent-final-archive-policy.json` |
| Independent actual-history privacy replay | Five API actions/two launches against disposable copies of a real completed writer archive | PASS; exact original historical bytes survive in the control, but current Read revocation/private exclusions hide them; deleted file stays absent |
| Authenticated conversation/pipeline | 14 actual provider requests and 13 actual tool calls, with workspace Process and Network disabled | PASS; `live-final20/acceptance.json`, settlement `Accepted` |
| Independent final CLI operation | Actual `cargo run --offline --quiet` on the generated source | PASS; exact stdout `After\n`, empty stderr |

That is **1,120 scripted real API actions**, separately from the authenticated model's work and
the resulting CLI operation. The independent native campaigns and main/reviewer consumers have
empty stderr. The live runtime log contains phase/progress observations; its final inspection and
generated CLI also have empty stderr. No observed relevant failure remains unresolved in this
bounded campaign. The [compact evidence record](read-loop-recovery-evidence.json) records the
final identities/counts; the preserved independent failure has its own original/final records.

The authenticated scenario uses the official account provider through the real ProductRunner,
not just context assembly. It starts a fresh disposable Rust CLI printing `Before`, reads an
approved external brief, follows the exact edit sequence, preserves unrelated files, and requests
independent review. Workspace Process/Network permissions are disabled. After acceptance the
external fixture owner runs `cargo run --offline --quiet` and requires exactly `After\n` with
empty stderr. A copied completed writer archive is reopened to check the historical temporary
read and no-op receipt. No live user's daemon or project is modified.

Builds use one Cargo worker, no dev debug data, and a systemd scope with `MemoryHigh=1800M`,
`MemoryMax=2400M`, `CPUQuota=200%`. The final live replay has `MemoryHigh=1200M`,
`MemoryMax=1800M`, `CPUQuota=150%` and a 900-second external observation deadline. These are
campaign resource controls, not new application task/token budgets.

## Reproduction, retained evidence, and boundaries

Portable consumers, generated inputs, and the independent saved failure are under
[reproducers/read-loop-recovery](reproducers/read-loop-recovery/README.md). The README gives exact
build/native/live replay commands; each replay requires a new disposable output directory and
records input, output, stderr, fixtures, consumer identity, and nonzero failure status.

Raw initial audit evidence is outside the repository at
`/home/doll/.cache/peritus-read-loop-audit-20261020/`; repair iterations and final artifacts are
under `/home/doll/.cache/peritus-read-loop-fix-20261020/`. Independent generated execution evidence
is under `/home/doll/.cache/peritus-read-loop-independent-20261020/`. Successful bulk archives and
private provider transcripts are intentionally kept outside Git. Distinct original failures and
the reusable generation/replay commands remain recorded here.

The repository-wide ripgrep investigation covered 387 focused matches across 94 files and followed
the relevant interactive callers, recovery, permissions, provider framing, and module wrappers.
Follow-up searches examined the repaired event/correction/measurement callers. This is targeted
control-flow coverage, not a statement that every source function was reviewed.

The campaign exercises native Linux public APIs, durable cold recovery, actual official-provider
orchestration, and the resulting CLI. No TUI/WebUI behavior changed here; new UI journeys,
Windows/macOS execution, every provider/model's discretionary behavior, and a multi-hour
autonomous run were not exercised. Invocation rollover is directly exercised through the real
lifecycle API rather than waiting for 48 discretionary model turns. The ledger removes host
rediscovery causes and avoids redelivering already-visible bodies; it cannot guarantee that every
model will always select a useful next action. No arbitrary read/task/token quota or forced
speculative edit was introduced.
