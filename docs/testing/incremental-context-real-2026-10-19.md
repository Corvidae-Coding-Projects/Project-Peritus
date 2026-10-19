# Incremental context and document reuse — real campaign, 2026-10-19

## Observable contract (defined before execution)

Operate the production public workspace tools and `LocalContextHandle` in disposable folders.
Do not submit work to either user's retained run. No unit or formal verification suites.

| Action | Required result |
| --- | --- |
| Repeat and overlap line/byte pages of an unchanged document | Exact requested text and continuation cursors; warm reads avoid rereading the document prefix. |
| Rewrite equal-length text and restore mtime; replace inode; delete/recreate | Subsequent read returns current bytes and digest, never the old cached body. |
| Change read authority, protect a path, or cancel after warming | Current rejection applies even with cached text. Restoring authority permits a fresh valid read. |
| Read CRLF, Unicode, empty lines, a long line, missing final newline, and invalid UTF-8 | Preserve existing line and byte-cursor semantics, including errors and continuation progress. |
| Exceed document retention capacity or read a larger file | Eviction/fallback preserves content; cache capacity never rejects an otherwise legal document. |
| Assemble unchanged context repeatedly, then append a settled tool exchange | No old messages reclassified; decoded immutable messages reused; only the suffix is classified. |
| Change source text after warming context | Cached old read becomes an obsolete-source receipt; unrelated settled results and call/result ordering survive. |
| Assemble while a tool proposal is pending, then settle it | Pending operation remains explicit and undispatched; the completed exchange appears exactly once after settlement. |
| Change current host policy/user instructions or begin another invocation | Current instructions win; old topology cannot migrate into the new invocation. |
| Close/reopen a lineage and replay the immediately affected saved regressions | Durable exact observations, scope isolation, source freshness and pending state remain valid. |

Measurements distinguish host IO/decoding from provider token caching. This change does not
alter provider adapters or promise vendor cache hits. Cache storage is disposable and bounded;
the durable journal and exact source archive remain authoritative. Cold archive recovery and
explicit artifact reads retain integrity checks. Cached typed message projections belong to
the locked lineage and do not cache permission, source-freshness, or acceptance decisions.

## Results

**PASS** for the production public-API consumers and immediately affected saved regressions.
The final application build completed with the pinned toolchain, `--locked`, dev debug disabled
and two jobs. CPU was capped at 200%, memory high at 2500 MiB and maximum at 3 GiB, with no
additional swap. The final rebuild took 14.47 s. No unit, formal or repository verification suite
was run; installed binaries and retained user runs were not changed.

| Actual production interface | Observed result |
| --- | --- |
| Seed 731944, 240 actions spanning the contract above | All passed, including 96 generated line ranges, long-line UTF-8 byte continuations, OS/host permission and cancellation changes, inode and restored-mtime changes, retention eviction and >16 MiB streaming. |
| Repeated ranges in an 822,385-byte Unicode/CRLF document | First read 1,644,880 logical bytes; maximum warm read 113. Counts include the consumer's `/proc/self/io` sample overhead. Exact text and source digest independently checked. |
| Original/current consumers: 96 ranges near the end of a 6,200,000-byte document | Exact public outputs identical. Logical read bytes fell from 578,561,474 to 12,419,272, about 97.9% less, including cold/process setup. |
| Original/current consumers: 24 changing context observations | Exact assembled model messages identical. Logical reads 43,120,717 to 33,876,587. Final publication, archive and journal work is still performed. |
| Six unchanged warmed context views | Zero decoded-cache misses and zero newly indexed messages; each appended settled call/result added exactly two indexed messages. |
| 48 large sources, 19,201,344 source bytes, enough to exceed decode retention; revisit source zero | 97 ordinary context views; 101 total decode misses (initial pins plus genuinely new messages), cold-view maximum five, later maximum two. Original intermediate implementation had 1,602 total and up to 103 misses/view. Exact latest source and call/result pairing preserved. |
| Change all 48 sources at equal length, restore mtimes, then assemble three times | All old bodies represented as obsolete receipts. Decode misses `[0, 0, 0]`; old payloads were not loaded just to generate small receipts. |
| Saved actual-context/recovery replays | Six cases passed: repeat suppression, fresh-process reuse, same-size/mtime refresh, revoked access, distinct scope isolation and stable history prefixes. |
| Saved mixed-batch/order regressions | Both passed: one stale read preserves another settled search; assistant text cannot reorder calls and results. |
| Independent context, separate fixture owner | 51 API requests and 68 checks in four fresh processes passed with empty stderr; final selection/estimate/source handling review found no concrete defect. |

Linux logical IO is not physical disk traffic, provider cache usage, or a billing measurement.
The final comparison ran alongside bounded exploration; wall times are supporting local evidence,
not an isolated portable performance benchmark. These are bounded Linux cases, not multi-hour
production or every-platform coverage.

## Failures discovered and resolved

The first decoded cache passed ordinary behavior but still materialized all historical exchanges
before selection. Once history exceeded retention, the cache churned: the 48-source replay had
1,602 decode misses and a peak of 103 in one view. The host now retains small append-only exchange
metadata, normalization hashes and composable input estimates; selection precedes loading. It
loads only selected still-relevant bodies. Stale-result metadata retains the original source
handle and replacement byte contribution, so even selected obsolete results can be constructed
without loading their old bodies. The strengthened pressure replay rejects the original churn.

The first `/tmp` pressure/native attempts hit `EDQUOT` despite filesystem free space. The actual
storage failures were retained separately; successful disk-backed replays establish the final
behavior. The initial dynamic-protection checker also confused a leave-alone write constraint
with a private read exclusion; it was corrected to rebind the actual private-path API. These
fixture/environment corrections did not loosen the promised read-permission behavior.

## Implementation and evidence

Document retention holds sparse line offsets and exact bytes behind file-identity/change tokens.
Each access still checks current policy and opens the file, including on a cache hit. Non-Unix
or coarse change clocks conservatively stream; larger files and evictions also stream normally.
The 64 MiB retention and 16 MiB per-document preference are cache memory choices, not read or run
limits. Decoded projections have a separate approximate 64 MiB global retention keyed by lineage
scope and immutable artifact digest. Cold loads use the artifact store's size/digest checks. The
append-only index contains only small topology/freshness/estimate metadata and resets when its
invocation/pins change; it is rebuilt on recovery rather than changing any durable format.

Content-free `context-view` events now record assembly elapsed time, newly indexed messages,
decoded-cache hits/misses and the preexisting input/prefix accounting. These counters distinguish
host reuse from vendor cache hits. The estimator's composable form uses the same underlying byte
weights as the original production estimator, adding framing once and rounding only after summing.

[Consumers and replay commands](reproducers/incremental-context/README.md) remain in the repository.
Raw owned evidence is outside it under `/home/doll/.cache/peritus-incremental-real-20261019/`;
the independent evidence is under `/home/doll/.cache/peritus-incremental-independent-20261019/`.
The temporary baseline/build area is `/tmp/peritus-incremental-real-20261019/`.

Final artifact SHA-256 values:

- Ordinary public consumer: `c8344680fe3460b2eff99e8dbb7dace785944a39c599c396dcaf0c8db307d4af`.
- Interactive public consumer: `961668c9717ea20c3152ef1d206d7c95546167a3624e611d7683d583c35a5c3a`.
- Current daemon used for the owned native journey: `ff71441cf3f14397b86b7c4d1279088f099ee6f072b99d3a6c9aa0d8bf84f2cc`.

## Native TUI/daemon journey

**PASS** through the actual TUI → daemon → official account-provider path. The disposable
run read two distinct source files, retained equal-worded observations with distinct provenance,
performed two real context updates, resolved/superseded old entries, retrieved their exact archived
state, and ended with the requested marker. The journey completed in 62.54 s. Active views omitted
terminal entries while exact retrieval retained them; source files were unchanged. The final
fixture daemon was the current rebuilt binary identified above. The driver closed its client and
gracefully shut down only its owned daemon. No installed binary or retained user task was updated.

The initial quota-affected native attempt is recorded as an environment failure, not a passing
application journey. The successful final run used disk-backed XDG directories and native TMPDIR.


