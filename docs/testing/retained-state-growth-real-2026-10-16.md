# Retained state keeps working as it grows — 2026-10-16

The contract is continued operation beyond the former local history and inventory quotas,
without deleting evidence, changing ownership, repeating effects, or merely raising a constant.
Provider restrictions and actual resource availability remain separate. No unit, mocked-provider,
formal, or repository verification suite was created or run.

## Changes

Default working-state observation, entry, text, reference, file-dependency and update-count
quotas are removed. Positive explicit library policies remain available. The production host
widens old retained policies without advancing semantic revisions or changing sources/entries.
Immutable observation pages and shared content avoid copying the entire observation history
on each reducer step; source lookup is indexed and graph checks use a queue.

Canonical state v2 uses wide logical counts and still reads v1. Production checkpoint schema 3
stores content-addressed working/source pages and publishes every page root in the existing C0
transaction. Schemas 1 and 2 remain readable with their original view-binding calculation.
A failed page decoder stays invalid; gaps, duplicates, trailing data, foreign roles and incomplete
snapshots cannot become a successful reconstruction.

Trace recovery streams complete frames, skips unrelated history, and retains a file-identity/
anchor-checked cursor. The 1-GiB total and 64-MiB scoped-frame quotas are removed. Journal
recovery verifies prefix records one at a time and retains only the uncovered suffix. Run-record
JSON is read/written through buffers with the existing fsync/rename publication; the 128-MiB
file quota is removed. Malformed data and torn effect boundaries remain explicit integrity errors.

In-place baselines use a transactional SQLite path index beside the original JSON journal.
Legacy before-images are imported once with earliest-stamp precedence. Enrollment streams
admissible descendants into one transaction and does not stop at 4,096 entries, depth 64, or
64 MiB of evidence. Protected paths, links, caches and runtime-node behavior remain distinct.

Local protocol request/event quotas now follow native field representation and selected
provider policies. This removes the former message/text/argument/event, JSON member and
ancillary metadata cutoffs, including the independent 64-MiB canonical event envelope.
Working views select durable, retrievable records within the provider envelope; they cannot
become an unshrinkable required closure. Large entry reference metadata has exact JSON
fragment pages as well as existing text pages. Omission accounting describes collapsed views.

## Reproduced defects and final results

Original baseline: develop `727cd41e6d44101147195c8d15bd3b8af3f86ef7`; protocol comparison
also used the retained original `66aa29f4039ec37d` public-library artifact. Each comparison
uses the actual consumer, canonical bytes or native executable; expected old failures do not
count as passing product behavior.

| Real operation and independent expectation | Original | Repaired |
| --- | --- | --- |
| Reuse exact v1 65,535-source snapshot, append source 65,536, retain every locator | Capacity failure | PASS |
| Decode/save/reopen 8,450,143-byte snapshot with 130,000 sources | Capacity failure | PASS |
| Preserve 512 entries while adding entry 513 | Capacity failure | PASS |
| Retain exact 9,216-byte Unicode entry | Capacity failure | PASS |
| Retain 33 source links, 513 actual file dependencies, and 33-operation atomic update | Three capacity failures | PASS |
| Snapshot pages: exact equality, missing/gap/duplicate/tamper/foreign-role/trailing rejection, poisoned decoder | Original paged API absent | PASS |
| 4,097 messages, text and JSON above 16 MiB, production event above 8 MiB, canonical event above 64 MiB | Five reproduced quota failures | PASS |
| 65,537 flat JSON members, 65 cache/rate observations, 65 rate windows, 129 provider events through actual codecs/reducer terminals | Fixed local cutoffs in original source | All five PASS |
| Admit 5,000 files, reopen, detect exact mutation, restore to unchanged | Directory too broad | PASS |
| Admit 66 nested directories plus leaf, reopen and compare | Directory too deep | PASS |
| Admit 4,090 files with 67,010,560 bytes of preview-bearing source, reopen and compare | Journal exceeds bound | PASS; SQLite index 69,414,912 bytes |
| Production context recovery from valid 1,073,900,912-byte trace | Recovery bound failure | PASS, then fresh-process schema-3 reopening |
| Copied closed long-run archive: preserve 104 entries and 7,054 sources, replay one uncovered event, publish/read paged checkpoint | Previously bounded retained policy | PASS; 47 exact entries visible, 57 explicitly paged, all 104 retained |
| Normal launcher/TUI and CLI reopen logically unchanged 134,251,481-byte completed record | Valid run quarantined by file size | PASS; visible completed run, exact ID and summary, no quarantine |
| Actual TUI/daemon/Codex: forty real source reads; one exact 33-operation update; 3,225-byte Unicode entry with forty real references; exact state pagination | Crosses old update/text/link limits | PASS: ten 2-KiB pages including four JSON fragments, all 33 entries exact |

The native memory journey used 20 actual provider requests and 55 tools in 264 seconds,
completed normally, left every project file unchanged, and shut down its owned daemon cleanly.
No command, build or test tools were invoked in that conversation. The large-record native
reopen completed in 0.506 seconds; it requested no inference or effects.

The C6 consumer used about 61 MiB maximum RSS at 130,000 observations. The 65-MiB event
roundtrip consumer used about 411 MiB; trace recovery/reopen used about 9 MiB, and the copied
brownfield host about 93 MiB. These are measured process snapshots, not a general leak or
unlimited-resource claim. The large trace contains validated physical JSON whitespace in a
complete historical metadata frame, not invalid sparse NUL holes.

## Regression discovered during this repair

The first indexed-scope replay passed 23 of 24 saved journeys but failed
`protected-cache-link-no-read`: explicit cache-file enrollment under a declared directory was
incorrectly treated as previously absent. Directory discovery intentionally excludes cache
contents, so its absence inference cannot govern a separately authorized exact cache file.
The correction restricts that inference to paths admitted to the parent inventory. The final
fresh-process replay passed all 24 journeys, including original before-images, legacy v1
imports, overlap order, Unicode/space paths, file-kind transitions, private/link/FIFO exclusion,
explicit cache capture and forty concurrent runtime observations. The three new growth
journeys also passed again after the correction.

Two native driver mistakes were corrected separately: the launcher can use a short `/tmp`
socket rather than a state-directory socket, and the CLI wire phase is `Complete` while the
TUI says “Completed in place.” Original raw results remain preserved. Neither mistake was
counted as a product failure or used to weaken the expected successful behavior.

## Reproduction and provenance

Use the [retained replay guide](reproducers/retained-state-growth/README.md). Compressed exact
original v1 inputs, manifest digests, generator/consumer sources and native drivers are retained.
The neighboring [scope sequence driver](reproducers/in-place-scope/README.md) was updated for
legacy inputs before migration and indexed baseline readback.

Build actual binaries with:

```sh
cargo build --locked --release --jobs 2 -p peritus-daemon -p peritus-cli -p peritus-tui
```

Tested native SHA-256 values:

| Binary | SHA-256 |
| --- | --- |
| peritus | af48e101668b0a4e5e6848f45493b04547678c06fde9702aace6cfcca12023dc |
| peritusd | fef7c346471249a9709ccf532bd3ccf17c0819fea7ef1965534b558ac3d56945 |
| peritus-tui | 49210219092525e05cb3f1965c5d141f3e888bfa177bde89bcc14d36f4bc010c |

Private raw evidence is under `/home/doll/Peritus-Builder-Repair/keep-working-followup/`;
owned native fixtures are `/home/builder/peritus-keep-working-*`. Test preparation used closed
backups and disposable project copies. The live `/home/builder/precient` job was not used as a
mutation fixture. Native diagnostics showed no new failure. All recorded final boundary,
sequence, persistence and real-provider scenarios passed. This is bounded Linux coverage;
other native platforms and remote providers were not independently exercised here.
